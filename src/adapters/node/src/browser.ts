/**
 * lpdf browser entry point (ESM).
 *
 * Unlike the Node.js entry, the browser cannot load files from disk and cannot
 * use `require()`.  The WASM binary must be served as a static asset.
 *
 * Usage:
 *
 *   import { initLpdf } from 'lpdf/browser'
 *
 *   const lpdf = await initLpdf(new URL('./lpdf_bg.wasm', import.meta.url), licenseKey)
 *   const pdfBytes = await lpdf.renderPdf(xmlString)
 *   const pdfBytes2 = await lpdf.renderPdf(xmlString2, { fontBytes: { ... } })
 *
 * Custom fonts must be pre-loaded and supplied via `fontBytes`; there is no
 * automatic filesystem fallback in a browser context.
 */
import initWasm, { LpdfEngine as WasmEngine } from '../../../../dist/web/lpdf.js';
import { buildPdf, RenderOptions, RenderTree } from './_shared';

export type { RenderOptions } from './_shared';
export type { LpdfDocument, LpdfPageNode, LpdfNode, LpdfContainerNode, LpdfTextNode, LpdfSpanNode, LpdfDividerNode,
              LpdfTokens, LpdfFontDef, LpdfMeta,
              StackInput, FlankInput, SplitInput, ClusterInput, GridInput, FrameInput, LinkInput,
              TextInput, SpanInput, DividerInput, PageInput, DocumentInput,
              StackOptions, FlankOptions, SplitOptions, ClusterOptions, GridOptions, FrameOptions, LinkOptions,
              TextOptions, SpanOptions, DividerOptions, PageOptions, DocumentOptions } from './kit';
export { LpdfKit } from './kit';

export interface LpdfBrowser {
  /**
   * Render an lpdf XML document to PDF bytes.
   * Custom fonts must be passed via `options.fontBytes`; src-path loading is
   * not available in the browser.
   */
  renderPdf(xml: string, options?: RenderOptions): Promise<Uint8Array>;
}

/**
 * Initialise the lpdf WASM engine and return a browser renderer.
 *
 * @param wasmSource  - URL, path string, `Response`, or raw WASM bytes used to
 *                      load `lpdf_bg.wasm`.  Typically:
 *                      `new URL('./lpdf_bg.wasm', import.meta.url)`
 * @param licenseKey  - License key. Omit or pass empty string for free tier
 *                      (watermark applied).
 * @param initOptions - Optional long-lived config (e.g. shared `fontBytes`).
 */
export async function initLpdf(
  wasmSource: Parameters<typeof initWasm>[0],
  licenseKey = '',
  initOptions: RenderOptions = {},
): Promise<LpdfBrowser> {
  await initWasm(wasmSource);

  return {
    async renderPdf(xml: string, callOptions: RenderOptions = {}): Promise<Uint8Array> {
      const fontBytes = { ...initOptions.fontBytes, ...callOptions.fontBytes };
      const engine = new WasmEngine(licenseKey);
      const raw = engine.render(xml);
      engine.free();

      const tree = JSON.parse(raw) as RenderTree;
      if (tree.error) throw new Error(tree.error);

      return buildPdf(tree, fontBytes);
    },
  };
}
