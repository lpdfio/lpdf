import { RenderOptions } from './_shared';
export type { RenderOptions } from './_shared';
/**
 * Stateful renderer. Construct once with the license key and optional shared
 * config; call `renderPdf` as many times as needed without repeating the key.
 */
export declare class Lpdf {
    private readonly _licenseKey;
    private readonly _opts;
    constructor(licenseKey: string, options?: RenderOptions);
    /**
     * Render an lpdf XML document to PDF bytes (Node.js).
     * Per-call `fontBytes` are merged with the instance-level ones; per-call
     * keys win on collision.  Custom fonts not supplied via `fontBytes` are
     * loaded from disk using the `src` path in the document's `<fonts>`
     * declaration.
     */
    renderPdf(xml: string, callOptions?: RenderOptions): Promise<Uint8Array>;
}
