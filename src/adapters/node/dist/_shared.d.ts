/**
 * Shared render-tree types used by both the Node.js and browser entry points.
 * PDF generation now happens inside the Rust core (pdf.rs) and is surfaced via
 * the WASM `render_pdf()` method — no pdf-lib dependency required.
 */
export interface RenderTree {
    version: number;
    meta: RenderMeta;
    pages: RenderPage[];
    watermark: {
        type: string;
        text: string;
        url?: string;
    } | null;
    error?: string;
}
export interface RenderMeta {
    title?: string;
    author?: string;
    subject?: string;
    keywords?: string[];
    creator?: string;
    fonts: Record<string, FontDef>;
}
export interface FontDef {
    builtin?: string;
    src?: string;
}
export interface RenderPage {
    width: number;
    height: number;
    background?: string;
    margin: [number, number, number, number];
    nodes: RenderNode[];
}
export type RenderNode = BoxNode | LineNode | TextNode | LinkNode;
export interface BoxNode {
    type: 'box';
    x: number;
    y: number;
    width: number;
    height: number;
    fill?: string;
    border_width: number;
    border_color?: string;
    radius: number;
    children: RenderNode[];
}
export interface LineNode {
    type: 'line';
    x1: number;
    y1: number;
    x2: number;
    y2: number;
    color: string;
    thickness: number;
}
export interface TextNode {
    type: 'text';
    x: number;
    y: number;
    content: string;
    font: string;
    size: number;
    color: string;
    text_align: string;
}
export interface LinkNode {
    type: 'link';
    url: string;
    x: number;
    y: number;
    width: number;
    height: number;
    children: RenderNode[];
}
export interface RenderOptions {
    /**
     * Pre-loaded font bytes for custom fonts referenced via <fonts src="…">.
     * Keys are the font names used in the document; values are raw TTF/OTF bytes.
     * @deprecated Pass font bytes via `LpdfEngine.loadFont()` instead.
     */
    fontBytes?: Record<string, Uint8Array>;
}
