import type { PageScope } from './_shared';
export interface StackOptions {
    gap?: string;
    padding?: string;
    background?: string;
    align?: string;
    justify?: string;
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
    end?: string;
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
    align?: string;
    equal?: string;
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
    justify?: string;
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
export interface TableOptions {
    cols: string;
    border?: string;
    stripe?: string;
    gap?: string;
    padding?: string;
    background?: string;
    width?: string;
    height?: string;
    repeat?: string;
    debug?: string;
}
export interface TheadOptions {
    background?: string;
}
export interface TrOptions {
    background?: string;
}
export interface TdOptions {
    padding?: string;
    background?: string;
    align?: string;
    valign?: string;
    border?: string;
    radius?: string;
    gap?: string;
    debug?: string;
}
export interface TextOptions {
    font?: string;
    fontSize?: string;
    textAlign?: string;
    color?: string;
    bold?: string;
    end?: string;
    padding?: string;
    background?: string;
    width?: string;
    height?: string;
    border?: string;
    radius?: string;
    repeat?: string;
    debug?: string;
}
export interface SpanOptions {
    font?: string;
    fontSize?: string;
    color?: string;
    bold?: string;
    url?: string;
    underline?: string;
    strike?: string;
}
export interface DividerOptions {
    color?: string;
    thickness?: string;
    direction?: string;
    debug?: string;
}
export interface ImgOptions {
    name: string;
    height?: string;
    width?: string;
    font?: string;
    fontSize?: string;
    gap?: string;
    padding?: string;
    background?: string;
    border?: string;
    radius?: string;
    repeat?: string;
    debug?: string;
}
export interface BarcodeOptions {
    type: string;
    data: string;
    size?: string;
    width?: string;
    height?: string;
    ec?: string;
    hrt?: string;
    color?: string;
    background?: string;
    repeat?: string;
    debug?: string;
}
export interface RegionOptions {
    page?: PageScope | string;
    w?: string;
    debug?: string;
}
export interface LpdfSpanNode {
    type: 'span';
    attrs: Record<string, string>;
    nodes: string[];
}
export interface LpdfContainerNode {
    type: 'stack' | 'flank' | 'split' | 'cluster' | 'grid' | 'frame' | 'link';
    attrs: Record<string, string>;
    nodes: LpdfNode[];
}
export interface LpdfTextNode {
    type: 'text';
    attrs: Record<string, string>;
    nodes: (string | LpdfSpanNode)[];
}
export interface LpdfDividerNode {
    type: 'divider';
    attrs: Record<string, string>;
}
export interface LpdfImgNode {
    type: 'img';
    attrs: Record<string, string>;
}
export interface LpdfBarcodeNode {
    type: 'barcode';
    attrs: Record<string, string>;
}
export interface LpdfTheadNode {
    type: 'thead';
    attrs: Record<string, string>;
    nodes: LpdfTdNode[];
}
export interface LpdfTrNode {
    type: 'tr';
    attrs: Record<string, string>;
    nodes: LpdfTdNode[];
}
export interface LpdfTdNode {
    type: 'td';
    attrs: Record<string, string>;
    nodes: LpdfNode[];
}
export interface LpdfTableNode {
    type: 'table';
    attrs: Record<string, string>;
    nodes: (LpdfTheadNode | LpdfTrNode)[];
}
export interface LpdfRegionNode {
    type: 'layout-region';
    attrs: Record<string, string>;
    nodes: LpdfNode[];
}
export type LpdfNode = LpdfContainerNode | LpdfTextNode | LpdfDividerNode | LpdfTableNode | LpdfImgNode | LpdfBarcodeNode | LpdfRegionNode;
declare function stack(nodes?: LpdfNode[], options?: StackOptions): LpdfContainerNode;
declare function flank(nodes?: LpdfNode[], options?: FlankOptions): LpdfContainerNode;
declare function split(nodes?: LpdfNode[], options?: SplitOptions): LpdfContainerNode;
declare function cluster(nodes?: LpdfNode[], options?: ClusterOptions): LpdfContainerNode;
declare function grid(nodes?: LpdfNode[], options?: GridOptions): LpdfContainerNode;
declare function frame(nodes?: LpdfNode[], options?: FrameOptions): LpdfContainerNode;
declare function link(nodes?: LpdfNode[], options?: LinkOptions): LpdfContainerNode;
declare function table(options: TableOptions, nodes?: (LpdfTheadNode | LpdfTrNode)[]): LpdfTableNode;
declare function thead(nodes?: LpdfTdNode[], options?: TheadOptions): LpdfTheadNode;
declare function tr(nodes?: LpdfTdNode[], options?: TrOptions): LpdfTrNode;
declare function td(nodes?: LpdfNode[], options?: TdOptions): LpdfTdNode;
declare function text(nodes?: (string | LpdfSpanNode)[], options?: TextOptions): LpdfTextNode;
declare function span(nodes?: string[], options?: SpanOptions): LpdfSpanNode;
declare function divider(options?: DividerOptions): LpdfDividerNode;
declare function img(options: ImgOptions): LpdfImgNode;
declare function barcode(options: BarcodeOptions): LpdfBarcodeNode;
declare function region(pin: string, nodes?: LpdfNode[], options?: RegionOptions): LpdfRegionNode;
export declare const LpdfLayout: Readonly<{
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
    img: typeof img;
    barcode: typeof barcode;
    table: typeof table;
    thead: typeof thead;
    tr: typeof tr;
    td: typeof td;
    region: typeof region;
}>;
export {};
