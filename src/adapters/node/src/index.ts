import { readFileSync } from 'node:fs';
import { RenderOptions } from './_shared';
import { LpdfDocument } from './kit';
import { kitToXml } from './kit-to-xml';

// The WASM CJS module is loaded at runtime; we declare only what we use.
interface IWasmEngine {
  render_pdf(xml: string): Uint8Array;
  load_font(name: string, bytes: Uint8Array): void;
  set_created_on(iso: string): void;
  free(): void;
}
interface WasmEngineConstructor { new(licenseKey: string): IWasmEngine; }
// require() path is relative to the compiled output at dist/index.js.
// dist/index.js → ../../../../dist/node/lpdf.js = project-root/dist/node/lpdf.js
// eslint-disable-next-line @typescript-eslint/no-require-imports
const { LpdfEngine: WasmEngine } = require('../../../../dist/node/lpdf.js') as { LpdfEngine: WasmEngineConstructor };

export type { RenderOptions } from './_shared';
export type { LpdfDocument, LpdfPageNode, LpdfNode, LpdfContainerNode, LpdfTextNode, LpdfSpanNode, LpdfDividerNode,
              LpdfTokens, LpdfFontDef, LpdfMeta,
              StackInput, FlankInput, SplitInput, ClusterInput, GridInput, FrameInput, LinkInput,
              TextInput, SpanInput, DividerInput, PageInput, DocumentInput,
              StackOptions, FlankOptions, SplitOptions, ClusterOptions, GridOptions, FrameOptions, LinkOptions,
              TextOptions, SpanOptions, DividerOptions, PageOptions, DocumentOptions } from './kit';
export { LpdfKit } from './kit';
export { kitToXml } from './kit-to-xml';

/**
 * Stateful renderer. Construct once with the license key and optional shared
 * config; call `renderPdf` as many times as needed without repeating the key.
 */
export class LpdfEngine {
  private readonly _licenseKey: string;
  private readonly _opts:  RenderOptions;
  private readonly _fonts: Map<string, Uint8Array> = new Map();
  private _disposed = false;

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
    // LpdfDocument trees are serialised to XML before being handed to the Rust engine.
    const xml = typeof input === 'string' ? input : kitToXml(input);

    // Merge fonts: instance-level loadFont() calls take precedence over the
    // deprecated fontBytes option, which is kept for one-version compat.
    const allFonts = new Map<string, Uint8Array>(this._fonts);
    const extraBytes = { ...this._opts.fontBytes, ...callOptions.fontBytes };
    for (const [name, bytes] of Object.entries(extraBytes)) {
      if (!allFonts.has(name)) allFonts.set(name, bytes);
    }

    // Auto-load fonts declared via <font src="…"> that haven't been
    // explicitly provided — mirrors the old srcFallback behaviour.
    for (const [name, src] of extractFontSrcs(xml)) {
      if (!allFonts.has(name)) {
        try { allFonts.set(name, readFileSync(src)); } catch { /* not found; Rust falls back to Helvetica */ }
      }
    }

    const engine = new WasmEngine(this._licenseKey);
    for (const [name, bytes] of allFonts) {
      engine.load_font(name, bytes);
    }
    const pdf = engine.render_pdf(xml);
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



