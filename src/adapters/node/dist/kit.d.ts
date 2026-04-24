/**
 * LpdfKit — document skeleton assembly.
 *
 * Assembles sections (containing layout and canvas blocks) into a document
 * tree ready for `LpdfEngine.renderPdf()` or `kitToXml()`.
 *
 * @example
 * ```ts
 * import { LpdfEngine, LpdfKit, LpdfLayout, LpdfCanvas } from 'lpdf';
 *
 * const doc = LpdfKit.document({
 *   sections: [
 *     LpdfKit.section({
 *       nodes: [
 *         LpdfKit.canvas([ LpdfCanvas.layer([ LpdfCanvas.rect(0, 0, 595, 842) ]) ]),
 *         LpdfKit.layout([ LpdfLayout.text(['Hello']) ]),
 *       ],
 *       options: { size: 'a4', margin: '28pt' },
 *     }),
 *   ],
 *   options: { meta: { title: 'My Doc' } },
 * });
 * const bytes = await new LpdfEngine(key).renderPdf(doc);
 * ```
 */
import type { LpdfNode } from './layout';
import type { LpdfCanvasLayerNode } from './canvas';
export interface LpdfTokens {
    colors?: Record<string, string>;
    space?: Record<string, string>;
    grid?: Record<string, string>;
    border?: Record<string, string>;
    radius?: Record<string, string>;
    width?: Record<string, string>;
    text?: Record<string, string>;
    fonts?: Record<string, LpdfFontDef>;
}
export type LpdfFontDef = {
    src: string;
    builtin?: never;
} | {
    builtin: string;
    src?: never;
};
export interface LpdfMeta {
    title?: string;
    author?: string;
    subject?: string;
    keywords?: string;
    creator?: string;
}
export interface SectionOptions {
    size?: string;
    orientation?: string;
    margin?: string;
    background?: string;
    title?: string;
    debug?: string;
}
export interface DocumentOptions {
    size?: string;
    orientation?: string;
    margin?: string;
    background?: string;
    tokens?: LpdfTokens;
    meta?: LpdfMeta;
    debug?: string;
}
/** A layout block — wraps an ordered list of layout nodes. */
export interface LpdfLayoutBlock {
    type: 'layout';
    nodes: LpdfNode[];
}
/** A canvas block — wraps an ordered list of canvas layers. */
export interface LpdfCanvasBlock {
    type: 'canvas';
    nodes: LpdfCanvasLayerNode[];
}
export interface LpdfSectionNode {
    type: 'section';
    attrs: Record<string, string>;
    nodes: (LpdfLayoutBlock | LpdfCanvasBlock)[];
}
export interface LpdfDocument {
    version: 1;
    type: 'document';
    attrs: Record<string, unknown>;
    nodes: LpdfSectionNode[];
}
export interface SectionInput {
    nodes?: (LpdfLayoutBlock | LpdfCanvasBlock)[];
    options?: SectionOptions;
}
export interface DocumentInput {
    /** Sections — serialised as `nodes` on the wire (matching kit_to_xml and parse_tree). */
    sections?: LpdfSectionNode[];
    options?: DocumentOptions;
}
declare function layout(nodes?: LpdfNode[]): LpdfLayoutBlock;
declare function canvas(layers?: LpdfCanvasLayerNode[]): LpdfCanvasBlock;
declare function section(input?: SectionInput): LpdfSectionNode;
declare function document(input?: DocumentInput): LpdfDocument;
export declare const LpdfKit: Readonly<{
    layout: typeof layout;
    canvas: typeof canvas;
    section: typeof section;
    document: typeof document;
}>;
export {};
