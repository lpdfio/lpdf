import { readFileSync } from 'node:fs';
import { buildPdf, RenderOptions, RenderTree } from './_shared';
import { LpdfDocument } from './kit';

// The WASM CJS module is loaded at runtime; we declare only what we use.
interface IWasmEngine { render(xml: string): string; render_tree?(json: string): string; free(): void; }
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
  private readonly _opts: RenderOptions;

  constructor(licenseKey: string, options: RenderOptions = {}) {
    this._licenseKey = licenseKey;
    this._opts = options;
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
    const fontBytes = { ...this._opts.fontBytes, ...callOptions.fontBytes };
    const engine = new WasmEngine(this._licenseKey);

    let raw: string;
    if (typeof input === 'string') {
      raw = engine.render(input);
    } else {
      if (!engine.render_tree) {
        throw new Error('renderTree is not supported by the current WASM build — update the lpdf core package.');
      }
      raw = engine.render_tree(JSON.stringify(input));
    }
    engine.free();

    const tree = JSON.parse(raw) as RenderTree;
    if (tree.error) throw new Error(tree.error);

    return buildPdf(tree, fontBytes, (path) => readFileSync(path));
  }
}



