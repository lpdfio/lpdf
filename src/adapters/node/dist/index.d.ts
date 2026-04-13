import { RenderOptions } from './_shared';
export type { RenderOptions } from './_shared';
/**
 * Render an lpdf XML document to PDF bytes (Node.js).
 * Custom fonts not supplied via `fontBytes` are loaded from disk using the
 * `src` path in the document's `<fonts>` declaration.
 */
export declare function renderPdf(xml: string, options?: RenderOptions): Promise<Uint8Array>;
