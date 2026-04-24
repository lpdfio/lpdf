import type { PageScope } from './_shared';
export type TextAlign = 'left' | 'center' | 'right' | 'justify';
export type LineCap = 'butt' | 'round' | 'square';
export type LineJoin = 'miter' | 'round' | 'bevel';
export interface CanvasRectStyle {
    fill?: string;
    stroke?: string;
    strokeWidth?: number;
    strokeDash?: number[];
    borderRadius?: number;
}
export interface CanvasLineStyle {
    stroke?: string;
    strokeWidth?: number;
    strokeDash?: number[];
    lineCap?: LineCap;
    lineJoin?: LineJoin;
}
export interface CanvasEllipseStyle {
    fill?: string;
    stroke?: string;
    strokeWidth?: number;
    strokeDash?: number[];
}
export interface CanvasPathStyle {
    fill?: string;
    stroke?: string;
    strokeWidth?: number;
    strokeDash?: number[];
    fillRuleEvenodd?: boolean;
    lineCap?: LineCap;
    lineJoin?: LineJoin;
}
export interface CanvasTextStyle {
    font?: string;
    size?: number;
    color?: string;
    align?: TextAlign;
    lineHeight?: number;
    width?: number;
}
export interface CanvasRun {
    text: string;
    font?: string;
    size?: number;
    color?: string;
}
export declare class CanvasTransform {
    readonly matrix: number[];
    constructor(matrix: number[]);
    /** Rotate clockwise by `degrees` around the origin, or around (cx, cy). */
    static rotate(degrees: number, cx?: number, cy?: number): CanvasTransform;
    /** Scale by sx (and optionally sy; defaults to sx for uniform scale). */
    static scale(sx: number, sy?: number): CanvasTransform;
    /** Translate by (tx, ty). */
    static translate(tx: number, ty: number): CanvasTransform;
    /**
     * Combine: apply `other` first, then `this`.
     * Equivalent to matrix multiplication: this × other.
     */
    then(other: CanvasTransform): CanvasTransform;
    /** Serialise to the `"matrix(a,b,c,d,e,f)"` string form that Rust `jattr()` reads. */
    toString(): string;
}
export interface LpdfCanvasRectNode {
    type: 'canvas-rect';
    attrs: Record<string, string>;
}
export interface LpdfCanvasLineNode {
    type: 'canvas-line';
    attrs: Record<string, string>;
}
export interface LpdfCanvasEllipseNode {
    type: 'canvas-ellipse';
    attrs: Record<string, string>;
}
export interface LpdfCanvasCircleNode {
    type: 'canvas-circle';
    attrs: Record<string, string>;
}
export interface LpdfCanvasPathNode {
    type: 'canvas-path';
    attrs: Record<string, string>;
}
export interface LpdfCanvasTextNode {
    type: 'canvas-text';
    text: string;
    attrs: Record<string, string>;
    runs?: {
        text: string;
        attrs: {
            font?: string;
            'font-size'?: string;
            color?: string;
        };
    }[];
}
export interface LpdfCanvasImgNode {
    type: 'canvas-img';
    attrs: Record<string, string>;
}
export type LpdfCanvasPrimitiveNode = LpdfCanvasRectNode | LpdfCanvasLineNode | LpdfCanvasEllipseNode | LpdfCanvasCircleNode | LpdfCanvasPathNode | LpdfCanvasTextNode | LpdfCanvasImgNode;
export interface CanvasLayerOptions {
    page?: PageScope | string;
    opacity?: number;
    transform?: CanvasTransform;
}
export interface LpdfCanvasLayerNode {
    type: 'canvas-layer';
    attrs: Record<string, string>;
    nodes: LpdfCanvasPrimitiveNode[];
}
declare function rect(x: number, y: number, w: number, h: number, style?: CanvasRectStyle): LpdfCanvasRectNode;
declare function line(x1: number, y1: number, x2: number, y2: number, style?: CanvasLineStyle): LpdfCanvasLineNode;
declare function ellipse(cx: number, cy: number, rx: number, ry: number, style?: CanvasEllipseStyle): LpdfCanvasEllipseNode;
declare function circle(cx: number, cy: number, r: number, style?: CanvasEllipseStyle): LpdfCanvasCircleNode;
declare function path(d: string, style?: CanvasPathStyle): LpdfCanvasPathNode;
declare function canvasText(x: number, y: number, content: string, style?: CanvasTextStyle, runs?: CanvasRun[]): LpdfCanvasTextNode;
declare function img(x: number, y: number, w: number, h: number, name: string): LpdfCanvasImgNode;
declare function layer(nodes: LpdfCanvasPrimitiveNode[], options?: CanvasLayerOptions): LpdfCanvasLayerNode;
export declare const LpdfCanvas: Readonly<{
    rect: typeof rect;
    line: typeof line;
    ellipse: typeof ellipse;
    circle: typeof circle;
    path: typeof path;
    text: typeof canvasText;
    img: typeof img;
    layer: typeof layer;
}>;
export {};
