import { readFileSync } from 'node:fs';
import type { RenderOptions } from './_shared';
import type { LpdfDocument } from './kit';

// The WASM CJS module is loaded at runtime; we declare only what we use.
interface IWasmEngine {
  render_pdf(xml: string, json_data?: string | null): Uint8Array;
  render_tree_pdf(json: string): Uint8Array;
  load_font(name: string, bytes: Uint8Array): void;
  load_image(name: string, bytes: Uint8Array): void;
  set_created_on(iso: string): void;
  set_encryption(user_password: string, owner_password: string, permissions_json: string): void;
  clear_encryption(): void;
  free(): void;
}
interface IWasmModule {
  LpdfEngine: new (licenseKey: string) => IWasmEngine;
  kit_to_xml: (json: string) => string;
}
// require() path is relative to the compiled output at dist/engine.js.
// In the published package, wasm-pack artifacts live in dist/wasm/.
// In the monorepo, `npm run build` copies them there from <root>/dist/node/.
// eslint-disable-next-line @typescript-eslint/no-require-imports
const wasmModule = require('./wasm/lpdf.js') as IWasmModule;
const WasmEngine = wasmModule.LpdfEngine;

/** Thrown when the lpdf engine returns a layout or parse error. */
export class LpdfRenderError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'LpdfRenderError';
  }
}

/** PDF permission flags for RC4-128 encryption. All flags default to `true` (allowed). */
export interface EncryptPermissions {
  print?:         boolean;
  modify?:        boolean;
  copy?:          boolean;
  annotate?:      boolean;
  fill_forms?:    boolean;
  accessibility?: boolean;
  assemble?:      boolean;
  print_hq?:      boolean;
}

/** RC4-128 encryption options passed to {@link LpdfEngine.setEncryption}. */
export interface EncryptOptions {
  /** Open password shown to readers. Empty string = no open password required. */
  userPassword:  string;
  /** Owner (permissions) password. Required; must be non-empty. */
  ownerPassword: string;
  /** Permission flags applied to the document. Omitted flags default to `true`. */
  permissions?:  EncryptPermissions;
}

export class LpdfEngine {
  private readonly _licenseKey: string;
  private readonly _opts:   RenderOptions;
  private readonly _fonts:  Map<string, Uint8Array> = new Map();
  private readonly _images: Map<string, Uint8Array> = new Map();
  private _disposed = false;
  private _encrypt: EncryptOptions | null = null;

  constructor(licenseKey: string, options: RenderOptions = {}) {
    this._licenseKey = licenseKey;
    this._opts = options;
  }

  /**
   * Register raw TTF/OTF bytes for a custom font name used in `<font src="…">`.
   * Call before `renderPdf`. Returns `this` for chaining.
   */
  loadFont(name: string, bytes: Uint8Array): this {
    this._throwIfDisposed();
    this._fonts.set(name, bytes);
    return this;
  }

  /**
   * Register raw image bytes (PNG or JPEG) for an image name used in `<img name="…">`.
   * Call before `renderPdf`. Returns `this` for chaining.
   */
  loadImage(name: string, bytes: Uint8Array): this {
    this._throwIfDisposed();
    this._images.set(name, bytes);
    return this;
  }

  /**
   * Configure RC4-128 encryption for all subsequent `renderPdf` calls.
   * Returns `this` for chaining.
   */
  setEncryption(options: EncryptOptions): this {
    this._throwIfDisposed();
    this._encrypt = options;
    return this;
  }

  /**
   * Remove any previously configured encryption.
   * Returns `this` for chaining.
   */
  clearEncryption(): this {
    this._throwIfDisposed();
    this._encrypt = null;
    return this;
  }

  /**
   * Release held resources. Idempotent. Subsequent `renderPdf` / `loadFont`
   * calls after disposal will throw.
   */
  dispose(): void {
    this._disposed = true;
  }

  [Symbol.dispose](): void { this.dispose(); }

  private _throwIfDisposed(): void {
    if (this._disposed) throw new Error('LpdfEngine has been disposed.');
  }

  /**
   * Render an lpdf XML string to PDF bytes (Node.js).
   */
  async renderPdf(input: string, callOptions?: RenderOptions): Promise<Uint8Array>;
  /**
   * Render an `LpdfDocument` tree (built with `LpdfKit`) to PDF bytes (Node.js).
   */
  async renderPdf(input: LpdfDocument, callOptions?: RenderOptions): Promise<Uint8Array>;
  async renderPdf(input: string | LpdfDocument, callOptions: RenderOptions = {}): Promise<Uint8Array> {
    this._throwIfDisposed();

    // Merge fonts: instance-level loadFont() calls take precedence over the
    // deprecated fontBytes option, which is kept for one-version compat.
    const allFonts = new Map<string, Uint8Array>(this._fonts);
    const extraBytes = { ...this._opts.fontBytes, ...callOptions.fontBytes };
    for (const [name, bytes] of Object.entries(extraBytes)) {
      if (!allFonts.has(name)) allFonts.set(name, bytes);
    }

    const engine = new WasmEngine(this._licenseKey);

    const createdOn = callOptions.createdOn ?? this._opts.createdOn;
    if (createdOn) {
      engine.set_created_on(createdOn);
    }

    let pdf: Uint8Array;
    try {
      if (typeof input === 'string') {
        // XML path — auto-load fonts and images declared via src="…".
        const xml = input;
        for (const [key, src] of extractAssetSrcs(xml, 'font')) {
          if (!allFonts.has(key)) {
            try { allFonts.set(key, readFileSync(src)); } catch { /* not found; Rust falls back to Helvetica */ }
          }
        }
        for (const [name, bytes] of allFonts) {
          engine.load_font(name, bytes);
        }
        for (const [name, bytes] of this._images) {
          engine.load_image(name, bytes);
        }
        // Auto-load images declared via <image src="…"> that weren't pre-loaded.
        for (const [key, src] of extractAssetSrcs(xml, 'image')) {
          if (!this._images.has(key)) {
            try { engine.load_image(key, readFileSync(src)); } catch { /* skip unresolvable image */ }
          }
        }
        if (this._encrypt) {
          const permsJson = JSON.stringify(this._encrypt.permissions ?? {});
          engine.set_encryption(this._encrypt.userPassword, this._encrypt.ownerPassword, permsJson);
        }
        const dataJson = callOptions.data != null ? JSON.stringify(callOptions.data) : null;
        pdf = engine.render_pdf(xml, dataJson);
      } else {
        // JSON (Kit tree) path — pass JSON directly to render_tree_pdf.
        const json = JSON.stringify(input);
        for (const [key, src] of extractFontSrcsFromJson(json)) {
          if (!allFonts.has(key)) {
            try { allFonts.set(key, readFileSync(src)); } catch { /* not found; Rust falls back to Helvetica */ }
          }
        }
        for (const [name, bytes] of allFonts) {
          engine.load_font(name, bytes);
        }
        for (const [name, bytes] of this._images) {
          engine.load_image(name, bytes);
        }
        if (this._encrypt) {
          const permsJson = JSON.stringify(this._encrypt.permissions ?? {});
          engine.set_encryption(this._encrypt.userPassword, this._encrypt.ownerPassword, permsJson);
        }
        pdf = engine.render_tree_pdf(json);
      }
    } catch (e) {
      engine.free();
      const msg = e instanceof Error ? e.message : String(e);
      throw new LpdfRenderError(msg);
    }
    engine.free();
    return pdf;
  }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/** Extract `ref??name → src` pairs from `<font>` or `<image>` tags in XML. */
function extractAssetSrcs(xml: string, tag: 'font' | 'image'): Map<string, string> {
  const result = new Map<string, string>();
  const re = tag === 'font' ? /<font\s[^>]*>/g : /<image\s[^>]*>/g;
  for (const match of xml.matchAll(re)) {
    const t    = match[0];
    const name = /\bname="([^"]*)"/.exec(t)?.[1];
    const ref  = /\bref="([^"]*)"/.exec(t)?.[1];
    const src  = /\bsrc="([^"]*)"/.exec(t)?.[1];
    const key  = ref ?? name;
    if (key && src) result.set(key, src);
  }
  return result;
}

/** Extract `ref??name → src` pairs from `attrs.tokens.fonts[name].src` in a kit JSON string. */
function extractFontSrcsFromJson(json: string): Map<string, string> {
  const result = new Map<string, string>();
  try {
    const doc = JSON.parse(json);
    const fonts = doc?.attrs?.tokens?.fonts ?? {};
    for (const [name, def] of Object.entries(fonts as Record<string, { ref?: string; src?: string }>)) {
      if (def.src) {
        const key = def.ref ?? name;
        result.set(key, def.src);
      }
    }
  } catch { /* ignore */ }
  return result;
}
