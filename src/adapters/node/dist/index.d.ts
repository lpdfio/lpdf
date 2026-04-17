import { RenderOptions } from './_shared';
import { LpdfDocument } from './kit';
export type { RenderOptions } from './_shared';
export type { LpdfDocument, LpdfPageNode, LpdfNode, LpdfContainerNode, LpdfTextNode, LpdfSpanNode, LpdfDividerNode, LpdfTokens, LpdfFontDef, LpdfMeta, StackInput, FlankInput, SplitInput, ClusterInput, GridInput, FrameInput, LinkInput, TextInput, SpanInput, DividerInput, PageInput, DocumentInput, StackOptions, FlankOptions, SplitOptions, ClusterOptions, GridOptions, FrameOptions, LinkOptions, TextOptions, SpanOptions, DividerOptions, PageOptions, DocumentOptions } from './kit';
export { LpdfKit } from './kit';
export { kitToXml } from './kit-to-xml';
/** Thrown when the lpdf engine returns a layout or parse error. */
export declare class LpdfRenderError extends Error {
    constructor(message: string);
}
/**
 * Stateful renderer. Construct once with the license key and optional shared
 * config; call `renderPdf` as many times as needed without repeating the key.
 */
export declare class LpdfEngine {
    private readonly _licenseKey;
    private readonly _opts;
    private readonly _fonts;
    private readonly _images;
    private _disposed;
    constructor(licenseKey: string, options?: RenderOptions);
    /**
     * Register raw TTF/OTF bytes for a custom font name used in `<font src="…">`.
     * Call before `renderPdf`. Returns `this` for chaining.
     */
    loadFont(name: string, bytes: Uint8Array): this;
    /**
     * Register raw image bytes (PNG or JPEG) for an image name used in `<img name="…">`.
     * Call before `renderPdf`. Returns `this` for chaining.
     */
    loadImage(name: string, bytes: Uint8Array): this;
    /**
     * Release held resources. Idempotent. Subsequent `renderPdf` / `loadFont`
     * calls after disposal will throw.
     */
    dispose(): void;
    [Symbol.dispose](): void;
    private _throwIfDisposed;
    /**
     * Render an lpdf XML string to PDF bytes (Node.js).
     */
    renderPdf(input: string, callOptions?: RenderOptions): Promise<Uint8Array>;
    /**
     * Render an `LpdfDocument` tree (built with `LpdfKit`) to PDF bytes (Node.js).
     */
    renderPdf(input: LpdfDocument, callOptions?: RenderOptions): Promise<Uint8Array>;
}
