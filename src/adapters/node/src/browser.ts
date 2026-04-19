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
 *   lpdf.loadFont('Inter', interFontBytes)
 *   const pdfBytes2 = await lpdf.renderPdf(xmlString2)
 *
 * Custom fonts must be pre-loaded via `loadFont()` or the deprecated
 * `fontBytes` option; there is no automatic filesystem fallback in the browser.
 */
import initWasm, { LpdfEngine as WasmEngine } from '../../../../dist/web/lpdf.js';
import { RenderOptions } from './_shared';

export type { RenderOptions } from './_shared';
export type { LpdfDocument, LpdfPageNode, LpdfNode, LpdfContainerNode, LpdfTextNode, LpdfSpanNode, LpdfDividerNode,
              LpdfImgNode, LpdfBarcodeNode,
              LpdfTokens, LpdfFontDef, LpdfMeta,
              StackInput, FlankInput, SplitInput, ClusterInput, GridInput, FrameInput, LinkInput,
              TextInput, SpanInput, DividerInput, ImgInput, BarcodeInput, PageInput, DocumentInput,
              StackOptions, FlankOptions, SplitOptions, ClusterOptions, GridOptions, FrameOptions, LinkOptions,
              TextOptions, SpanOptions, DividerOptions, ImgOptions, BarcodeOptions, PageOptions, DocumentOptions } from './kit';
export { LpdfKit } from './kit';

export interface LpdfBrowser {
  /**
   * Register raw TTF/OTF bytes for a custom font name used in `<font src="…">`.
   * Call before `renderPdf`. Mutates the renderer in-place.
   */
  loadFont(name: string, bytes: Uint8Array): void;
  /**
   * Register raw image bytes (PNG or JPEG) for an image name used in `<img name="…">`.
   * Call before `renderPdf`. Mutates the renderer in-place.
   */
  loadImage(name: string, bytes: Uint8Array): void;
  /**
   * Render an lpdf XML document to PDF bytes.
   * Custom fonts must be registered via `loadFont()` or the deprecated
   * `fontBytes` option; src-path loading is not available in the browser.
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
  wasmSource:  Parameters<typeof initWasm>[0],
  licenseKey = '',
  initOptions: RenderOptions = {},
): Promise<LpdfBrowser> {
  await initWasm(wasmSource);

  const fontMap  = new Map<string, Uint8Array>();
  const imageMap = new Map<string, Uint8Array>();

  // Seed with any fonts supplied at init time (deprecated fontBytes path).
  if (initOptions.fontBytes) {
    for (const [name, bytes] of Object.entries(initOptions.fontBytes)) {
      fontMap.set(name, bytes);
    }
  }

  return {
    loadFont(name: string, bytes: Uint8Array): void {
      fontMap.set(name, bytes);
    },

    loadImage(name: string, bytes: Uint8Array): void {
      imageMap.set(name, bytes);
    },

    async renderPdf(xml: string, callOptions: RenderOptions = {}): Promise<Uint8Array> {
      const engine = new WasmEngine(licenseKey);

      for (const [name, bytes] of fontMap) {
        engine.load_font(name, bytes);
      }
      // Per-call fontBytes (deprecated) take lower precedence than loadFont().
      if (callOptions.fontBytes) {
        for (const [name, bytes] of Object.entries(callOptions.fontBytes)) {
          if (!fontMap.has(name)) engine.load_font(name, bytes);
        }
      }

      for (const [name, bytes] of imageMap) {
        engine.load_image(name, bytes);
      }

      const dataJson = callOptions.data != null ? JSON.stringify(callOptions.data) : null;
      const pdf = engine.render_pdf(xml, dataJson);
      engine.free();
      return pdf;
    },
  };
}
