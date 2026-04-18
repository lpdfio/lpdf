import { readFileSync } from 'node:fs';
import { RenderOptions } from './_shared';
import { LpdfDocument } from './kit';

// The WASM CJS module is loaded at runtime; we declare only what we use.
interface IWasmEngine {
  render_pdf(xml: string): Uint8Array;
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
interface WasmEngineConstructor { new(licenseKey: string): IWasmEngine; }
// require() path is relative to the compiled output at dist/index.js.
// dist/index.js → ../../../../dist/node/lpdf.js = project-root/dist/node/lpdf.js
// eslint-disable-next-line @typescript-eslint/no-require-imports
const wasmModule = require('../../../../dist/node/lpdf.js') as IWasmModule;
const WasmEngine = wasmModule.LpdfEngine;

export type { RenderOptions } from './_shared';
export type { LpdfDocument, LpdfPageNode, LpdfNode, LpdfContainerNode, LpdfTextNode, LpdfSpanNode, LpdfDividerNode,
              LpdfImgNode, LpdfBarcodeNode,
              LpdfTokens, LpdfFontDef, LpdfMeta,
              StackInput, FlankInput, SplitInput, ClusterInput, GridInput, FrameInput, LinkInput,
              TextInput, SpanInput, DividerInput, ImgInput, BarcodeInput, PageInput, DocumentInput,
              StackOptions, FlankOptions, SplitOptions, ClusterOptions, GridOptions, FrameOptions, LinkOptions,
              TextOptions, SpanOptions, DividerOptions, ImgOptions, BarcodeOptions, PageOptions, DocumentOptions } from './kit';
export { LpdfKit } from './kit';
export { kitToXml } from './kit-to-xml';

/** Thrown when the lpdf engine returns a layout or parse error. */
export class LpdfRenderError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'LpdfRenderError';
  }
}

/**
 * Stateful renderer. Construct once with the license key and optional shared
 * config; call `renderPdf` as many times as needed without repeating the key.
 */
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
        // XML path — auto-load fonts declared via <font src="…">.
        const xml = input;
        for (const [name, src] of extractFontSrcs(xml)) {
          if (!allFonts.has(name)) {
            try { allFonts.set(name, readFileSync(src)); } catch { /* not found; Rust falls back to Helvetica */ }
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
        pdf = engine.render_pdf(xml);
      } else {
        // JSON (Kit tree) path — pass JSON directly to render_tree_pdf.
        const json = JSON.stringify(input);
        for (const [name, src] of extractFontSrcsFromJson(json)) {
          if (!allFonts.has(name)) {
            try { allFonts.set(name, readFileSync(src)); } catch { /* not found; Rust falls back to Helvetica */ }
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

/** Extract `name → src` pairs from `<font name="…" src="…">` tags in XML. */
function extractFontSrcs(xml: string): Map<string, string> {
  const result = new Map<string, string>();
  for (const match of xml.matchAll(/<font\s[^>]*>/g)) {
    const tag  = match[0];
    const name = /\bname="([^"]*)"/.exec(tag)?.[1];
    const src  = /\bsrc="([^"]*)"/.exec(tag)?.[1];
    if (name && src) result.set(name, src);
  }
  return result;
}

/** Extract `name → src` pairs from `attrs.tokens.fonts[name].src` in a kit JSON string. */
function extractFontSrcsFromJson(json: string): Map<string, string> {
  const result = new Map<string, string>();
  try {
    const doc = JSON.parse(json);
    const fonts = doc?.attrs?.tokens?.fonts ?? {};
    for (const [name, def] of Object.entries(fonts as Record<string, { src?: string }>)) {
      if (def.src) result.set(name, def.src);
    }
  } catch { /* ignore */ }
  return result;
}

