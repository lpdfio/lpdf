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
 *   const lpdf = await initLpdf(new URL('./lpdf_bg.wasm', import.meta.url))
 *   const pdfBytes = await lpdf.renderPdf(xmlString, { licenseKey: '...' })
 *
 * Custom fonts must be pre-loaded and supplied via `fontBytes`; there is no
 * automatic filesystem fallback in a browser context.
 */
import initWasm, { LpdfEngine } from '../../../../dist/web/lpdf.js';
import { buildPdf, RenderOptions, RenderTree } from './_shared';

export type { RenderOptions } from './_shared';

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
 * @param wasmSource - URL, path string, `Response`, or raw WASM bytes used to
 *                     load `lpdf_bg.wasm`.  Typically:
 *                     `new URL('./lpdf_bg.wasm', import.meta.url)`
 */
export async function initLpdf(
  wasmSource: Parameters<typeof initWasm>[0],
): Promise<LpdfBrowser> {
  await initWasm(wasmSource);

  return {
    async renderPdf(xml: string, options: RenderOptions = {}): Promise<Uint8Array> {
      const engine = new LpdfEngine(options.licenseKey ?? '');
      const raw = engine.render(xml);
      engine.free();

      const tree = JSON.parse(raw) as RenderTree;
      if (tree.error) throw new Error(tree.error);

      return buildPdf(tree, options.fontBytes ?? {});
    },
  };
}
