/**
 * Shared render-tree types and pdf-lib drawing logic used by both the Node.js
 * and browser entry points.  No Node built-ins (no `node:fs`, no `require`).
 */
import { PDFDocument, PDFFont, Color } from 'pdf-lib';
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
     */
    fontBytes?: Record<string, Uint8Array>;
}
/**
 * Embed all fonts referenced by the render tree into the PDF document.
 *
 * @param srcFallback  Optional callback to load font bytes by path (Node only).
 *                     In the browser this is left undefined; callers must
 *                     supply all custom font bytes via `providedBytes`.
 */
export declare function loadFonts(doc: PDFDocument, tree: RenderTree, providedBytes: Record<string, Uint8Array>, srcFallback?: (path: string) => Uint8Array): Promise<Map<string, PDFFont>>;
export declare function buildPdf(tree: RenderTree, providedBytes: Record<string, Uint8Array>, srcFallback?: (path: string) => Uint8Array): Promise<Uint8Array>;
export declare function parseHex(hex: string): Color;
