/**
 * LpdfKit — tree-builder helpers for constructing lpdf document trees
 * programmatically.
 *
 * All helpers return plain serialisable objects. The resulting tree is passed
 * to `LpdfEngine.renderPdf(doc)`.
 *
 * @example
 * ```ts
 * import { LpdfEngine, LpdfKit } from 'lpdf';
 *
 * const doc = LpdfKit.document({
 *   nodes: [LpdfKit.page({ nodes: [LpdfKit.text({ nodes: ['Hello'] })] })],
 *   options: { meta: { title: 'My Doc' } },
 * });
 * const bytes = await new LpdfEngine(key).renderPdf(doc);
 * ```
 */
export interface StackOptions {
    gap?: string;
    padding?: string;
    background?: string;
    align?: string;
    width?: string;
    height?: string;
    border?: string;
    radius?: string;
    debug?: string;
}
export interface FlankOptions {
    gap?: string;
    padding?: string;
    background?: string;
    align?: string;
    justify?: string;
    wrap?: string;
    width?: string;
    height?: string;
    border?: string;
    radius?: string;
    debug?: string;
}
export interface SplitOptions {
    gap?: string;
    padding?: string;
    background?: string;
    width?: string;
    height?: string;
    border?: string;
    radius?: string;
    debug?: string;
}
export interface ClusterOptions {
    gap?: string;
    padding?: string;
    background?: string;
    align?: string;
    width?: string;
    height?: string;
    border?: string;
    radius?: string;
    debug?: string;
}
export interface GridOptions {
    cols?: string;
    colWidth?: string;
    gap?: string;
    equal?: string;
    padding?: string;
    background?: string;
    width?: string;
    height?: string;
    border?: string;
    radius?: string;
    debug?: string;
}
export interface FrameOptions {
    width?: string;
    height?: string;
    padding?: string;
    background?: string;
    border?: string;
    radius?: string;
    align?: string;
    debug?: string;
}
export interface LinkOptions {
    url?: string;
    width?: string;
    height?: string;
    debug?: string;
}
export interface TextOptions {
    font?: string;
    fontSize?: string;
    textAlign?: string;
    color?: string;
    bold?: string;
    padding?: string;
    background?: string;
    repeat?: string;
    debug?: string;
}
export interface SpanOptions {
    font?: string;
    fontSize?: string;
    color?: string;
    bold?: string;
    url?: string;
}
export interface DividerOptions {
    color?: string;
    thickness?: string;
    direction?: string;
    debug?: string;
}
export interface PageOptions {
    size?: string;
    orientation?: string;
    margin?: string;
    background?: string;
    debug?: string;
}
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
export interface DocumentOptions {
    size?: string;
    orientation?: string;
    margin?: string;
    background?: string;
    tokens?: LpdfTokens;
    meta?: LpdfMeta;
    debug?: string;
}
export interface StackInput {
    nodes?: LpdfNode[];
    options?: StackOptions;
}
export interface FlankInput {
    nodes?: LpdfNode[];
    options?: FlankOptions;
}
export interface SplitInput {
    nodes?: LpdfNode[];
    options?: SplitOptions;
}
export interface ClusterInput {
    nodes?: LpdfNode[];
    options?: ClusterOptions;
}
export interface GridInput {
    nodes?: LpdfNode[];
    options?: GridOptions;
}
export interface FrameInput {
    nodes?: LpdfNode[];
    options?: FrameOptions;
}
export interface LinkInput {
    nodes?: LpdfNode[];
    options?: LinkOptions;
}
export interface TextInput {
    nodes?: (string | LpdfSpanNode)[];
    options?: TextOptions;
}
export interface SpanInput {
    nodes?: string[];
    options?: SpanOptions;
}
export interface DividerInput {
    options?: DividerOptions;
}
export interface PageInput {
    nodes?: LpdfNode[];
    options?: PageOptions;
}
export interface DocumentInput {
    nodes?: LpdfPageNode[];
    options?: DocumentOptions;
}
/** Spans are only valid as children of `LpdfTextNode` — not part of `LpdfNode`. */
export interface LpdfSpanNode {
    type: 'span';
    attrs: Record<string, string>;
    children: string[];
}
export interface LpdfContainerNode {
    type: 'stack' | 'flank' | 'split' | 'cluster' | 'grid' | 'frame' | 'link';
    attrs: Record<string, string>;
    children: LpdfNode[];
}
export interface LpdfTextNode {
    type: 'text';
    attrs: Record<string, string>;
    children: (string | LpdfSpanNode)[];
}
export interface LpdfDividerNode {
    type: 'divider';
    attrs: Record<string, string>;
}
export type LpdfNode = LpdfContainerNode | LpdfTextNode | LpdfDividerNode;
export interface LpdfPageNode {
    type: 'page';
    attrs: Record<string, string>;
    children: LpdfNode[];
}
export interface LpdfDocument {
    version: 1;
    type: 'document';
    attrs: Record<string, unknown>;
    children: LpdfPageNode[];
}
declare function stack(input?: StackInput): LpdfContainerNode;
declare function flank(input?: FlankInput): LpdfContainerNode;
declare function split(input?: SplitInput): LpdfContainerNode;
declare function cluster(input?: ClusterInput): LpdfContainerNode;
declare function grid(input?: GridInput): LpdfContainerNode;
declare function frame(input?: FrameInput): LpdfContainerNode;
declare function link(input?: LinkInput): LpdfContainerNode;
declare function text(input?: TextInput): LpdfTextNode;
declare function span(input?: SpanInput): LpdfSpanNode;
declare function divider(input?: DividerInput): LpdfDividerNode;
declare function page(input?: PageInput): LpdfPageNode;
declare function document(input?: DocumentInput): LpdfDocument;
/**
 * Static builder kit — plain frozen object, not a class.
 * Import alongside `LpdfEngine`:
 *
 * ```ts
 * import { LpdfEngine, LpdfKit } from 'lpdf';
 * ```
 */
export declare const LpdfKit: Readonly<{
    stack: typeof stack;
    flank: typeof flank;
    split: typeof split;
    cluster: typeof cluster;
    grid: typeof grid;
    frame: typeof frame;
    link: typeof link;
    text: typeof text;
    span: typeof span;
    divider: typeof divider;
    page: typeof page;
    document: typeof document;
}>;
export {};
