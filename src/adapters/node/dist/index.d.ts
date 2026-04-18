import { RenderOptions } from './_shared';
import { LpdfDocument } from './kit';
export type { RenderOptions } from './_shared';
export type { LpdfDocument, LpdfPageNode, LpdfNode, LpdfContainerNode, LpdfTextNode, LpdfSpanNode, LpdfDividerNode, LpdfImgNode, LpdfBarcodeNode, LpdfTokens, LpdfFontDef, LpdfMeta, StackInput, FlankInput, SplitInput, ClusterInput, GridInput, FrameInput, LinkInput, TextInput, SpanInput, DividerInput, ImgInput, BarcodeInput, PageInput, DocumentInput, StackOptions, FlankOptions, SplitOptions, ClusterOptions, GridOptions, FrameOptions, LinkOptions, TextOptions, SpanOptions, DividerOptions, ImgOptions, BarcodeOptions, PageOptions, DocumentOptions } from './kit';
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
/** PDF permission flags for RC4-128 encryption. All flags default to `true` (allowed). */
export interface EncryptPermissions {
    print?: boolean;
    modify?: boolean;
    copy?: boolean;
    annotate?: boolean;
    fill_forms?: boolean;
    accessibility?: boolean;
    assemble?: boolean;
    print_hq?: boolean;
}
/** RC4-128 encryption options passed to {@link LpdfEngine.setEncryption}. */
export interface EncryptOptions {
    /** Open password shown to readers. Empty string = no open password required. */
    userPassword: string;
    /** Owner (permissions) password. Required; must be non-empty. */
    ownerPassword: string;
    /** Permission flags applied to the document. Omitted flags default to `true`. */
    permissions?: EncryptPermissions;
}
export declare class LpdfEngine {
    private readonly _licenseKey;
    private readonly _opts;
    private readonly _fonts;
    private readonly _images;
    private _disposed;
    private _encrypt;
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
     * Configure RC4-128 encryption for all subsequent `renderPdf` calls.
     * Returns `this` for chaining.
     */
    setEncryption(options: EncryptOptions): this;
    /**
     * Remove any previously configured encryption.
     * Returns `this` for chaining.
     */
    clearEncryption(): this;
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
