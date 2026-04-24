import type { PageScope } from './_shared';

// ── Enum / const types ────────────────────────────────────────────────────────

export type TextAlign = 'left' | 'center' | 'right' | 'justify';
export type LineCap   = 'butt' | 'round' | 'square';
export type LineJoin  = 'miter' | 'round' | 'bevel';

// ── Style interfaces ──────────────────────────────────────────────────────────

export interface CanvasRectStyle {
  fill?:         string;
  stroke?:       string;
  strokeWidth?:  number;
  strokeDash?:   number[];
  borderRadius?: number;
}

export interface CanvasLineStyle {
  stroke?:      string;
  strokeWidth?: number;
  strokeDash?:  number[];
  lineCap?:     LineCap;
  lineJoin?:    LineJoin;
}

export interface CanvasEllipseStyle {
  fill?:        string;
  stroke?:      string;
  strokeWidth?: number;
  strokeDash?:  number[];
}

export interface CanvasPathStyle {
  fill?:            string;
  stroke?:          string;
  strokeWidth?:     number;
  strokeDash?:      number[];
  fillRuleEvenodd?: boolean;
  lineCap?:         LineCap;
  lineJoin?:        LineJoin;
}

export interface CanvasTextStyle {
  font?:       string;
  size?:       number;
  color?:      string;
  align?:      TextAlign;
  lineHeight?: number;
  width?:      number;
}

// ── Run ───────────────────────────────────────────────────────────────────────

export interface CanvasRun {
  text:   string;
  font?:  string;
  size?:  number;
  color?: string;
}

// ── CanvasTransform ───────────────────────────────────────────────────────────

export class CanvasTransform {
  readonly matrix: number[];

  constructor(matrix: number[]) {
    this.matrix = matrix;
  }

  /** Rotate clockwise by `degrees` around the origin, or around (cx, cy). */
  static rotate(degrees: number, cx = 0, cy = 0): CanvasTransform {
    const rad = (degrees * Math.PI) / 180;
    const cos = Math.cos(rad);
    const sin = Math.sin(rad);
    const e = cx - cx * cos + cy * sin;
    const f = cy - cx * sin - cy * cos;
    return new CanvasTransform([cos, sin, -sin, cos, e, f]);
  }

  /** Scale by sx (and optionally sy; defaults to sx for uniform scale). */
  static scale(sx: number, sy?: number): CanvasTransform {
    return new CanvasTransform([sx, 0, 0, sy ?? sx, 0, 0]);
  }

  /** Translate by (tx, ty). */
  static translate(tx: number, ty: number): CanvasTransform {
    return new CanvasTransform([1, 0, 0, 1, tx, ty]);
  }

  /**
   * Combine: apply `other` first, then `this`.
   * Equivalent to matrix multiplication: this × other.
   */
  then(other: CanvasTransform): CanvasTransform {
    const [a1, b1, c1, d1, e1, f1] = this.matrix;
    const [a2, b2, c2, d2, e2, f2] = other.matrix;
    return new CanvasTransform([
      a1 * a2 + c1 * b2,
      b1 * a2 + d1 * b2,
      a1 * c2 + c1 * d2,
      b1 * c2 + d1 * d2,
      a1 * e2 + c1 * f2 + e1,
      b1 * e2 + d1 * f2 + f1,
    ]);
  }

  /** Serialise to the `"matrix(a,b,c,d,e,f)"` string form that Rust `jattr()` reads. */
  toString(): string {
    return `matrix(${this.matrix.join(',')})`;
  }
}

// ── Canvas node interfaces ────────────────────────────────────────────────────

export interface LpdfCanvasRectNode {
  type:  'canvas-rect';
  attrs: Record<string, string>;
}

export interface LpdfCanvasLineNode {
  type:  'canvas-line';
  attrs: Record<string, string>;
}

export interface LpdfCanvasEllipseNode {
  type:  'canvas-ellipse';
  attrs: Record<string, string>;
}

export interface LpdfCanvasCircleNode {
  type:  'canvas-circle';
  attrs: Record<string, string>;
}

export interface LpdfCanvasPathNode {
  type:  'canvas-path';
  attrs: Record<string, string>;
}

export interface LpdfCanvasTextNode {
  type:  'canvas-text';
  text:  string;
  attrs: Record<string, string>;
  runs?: { text: string; attrs: { font?: string; 'font-size'?: string; color?: string } }[];
}

export interface LpdfCanvasImgNode {
  type:  'canvas-img';
  attrs: Record<string, string>;
}

export type LpdfCanvasPrimitiveNode =
  | LpdfCanvasRectNode
  | LpdfCanvasLineNode
  | LpdfCanvasEllipseNode
  | LpdfCanvasCircleNode
  | LpdfCanvasPathNode
  | LpdfCanvasTextNode
  | LpdfCanvasImgNode;

// ── Layer ─────────────────────────────────────────────────────────────────────

export interface CanvasLayerOptions {
  page?:      PageScope | string;
  opacity?:   number;
  transform?: CanvasTransform;
  // clip is not supported — Rust CanvasLayer has no clip field
}

export interface LpdfCanvasLayerNode {
  type:  'canvas-layer';
  attrs: Record<string, string>;
  nodes: LpdfCanvasPrimitiveNode[];
}

// ── LpdfCanvas factory ────────────────────────────────────────────────────────

function rect(x: number, y: number, w: number, h: number, style?: CanvasRectStyle): LpdfCanvasRectNode {
  const attrs: Record<string, string> = {
    x: String(x), y: String(y), w: String(w), h: String(h),
  };
  if (style?.fill         !== undefined) attrs['fill']         = style.fill;
  if (style?.stroke       !== undefined) attrs['stroke']       = style.stroke;
  if (style?.strokeWidth  !== undefined) attrs['stroke-width'] = String(style.strokeWidth);
  if (style?.strokeDash   !== undefined) attrs['stroke-dash']  = style.strokeDash.join(' ');
  if (style?.borderRadius !== undefined) attrs['radius']       = String(style.borderRadius);
  return { type: 'canvas-rect', attrs };
}

function line(x1: number, y1: number, x2: number, y2: number, style?: CanvasLineStyle): LpdfCanvasLineNode {
  const attrs: Record<string, string> = {
    x1: String(x1), y1: String(y1), x2: String(x2), y2: String(y2),
  };
  if (style?.stroke      !== undefined) attrs['stroke']       = style.stroke;
  if (style?.strokeWidth !== undefined) attrs['stroke-width'] = String(style.strokeWidth);
  if (style?.strokeDash  !== undefined) attrs['stroke-dash']  = style.strokeDash.join(' ');
  if (style?.lineCap     !== undefined) attrs['line-cap']     = style.lineCap;
  if (style?.lineJoin    !== undefined) attrs['line-join']    = style.lineJoin;
  return { type: 'canvas-line', attrs };
}

function ellipse(cx: number, cy: number, rx: number, ry: number, style?: CanvasEllipseStyle): LpdfCanvasEllipseNode {
  const attrs: Record<string, string> = {
    cx: String(cx), cy: String(cy), rx: String(rx), ry: String(ry),
  };
  if (style?.fill        !== undefined) attrs['fill']         = style.fill;
  if (style?.stroke      !== undefined) attrs['stroke']       = style.stroke;
  if (style?.strokeWidth !== undefined) attrs['stroke-width'] = String(style.strokeWidth);
  if (style?.strokeDash  !== undefined) attrs['stroke-dash']  = style.strokeDash.join(' ');
  return { type: 'canvas-ellipse', attrs };
}

function circle(cx: number, cy: number, r: number, style?: CanvasEllipseStyle): LpdfCanvasCircleNode {
  const attrs: Record<string, string> = {
    cx: String(cx), cy: String(cy), r: String(r),
  };
  if (style?.fill        !== undefined) attrs['fill']         = style.fill;
  if (style?.stroke      !== undefined) attrs['stroke']       = style.stroke;
  if (style?.strokeWidth !== undefined) attrs['stroke-width'] = String(style.strokeWidth);
  if (style?.strokeDash  !== undefined) attrs['stroke-dash']  = style.strokeDash.join(' ');
  return { type: 'canvas-circle', attrs };
}

function path(d: string, style?: CanvasPathStyle): LpdfCanvasPathNode {
  const attrs: Record<string, string> = { d };
  if (style?.fill        !== undefined) attrs['fill']         = style.fill;
  if (style?.stroke      !== undefined) attrs['stroke']       = style.stroke;
  if (style?.strokeWidth !== undefined) attrs['stroke-width'] = String(style.strokeWidth);
  if (style?.strokeDash  !== undefined) attrs['stroke-dash']  = style.strokeDash.join(' ');
  if (style?.fillRuleEvenodd !== undefined) attrs['fill-rule'] = style.fillRuleEvenodd ? 'evenodd' : 'nonzero';
  if (style?.lineCap     !== undefined) attrs['line-cap']     = style.lineCap;
  if (style?.lineJoin    !== undefined) attrs['line-join']    = style.lineJoin;
  return { type: 'canvas-path', attrs };
}

function canvasText(
  x: number, y: number, content: string,
  style?: CanvasTextStyle,
  runs?: CanvasRun[],
): LpdfCanvasTextNode {
  const attrs: Record<string, string> = { x: String(x), y: String(y) };
  if (style?.font       !== undefined) attrs['font']        = style.font;
  if (style?.size       !== undefined) attrs['font-size']   = String(style.size);
  if (style?.color      !== undefined) attrs['color']       = style.color;
  if (style?.align      !== undefined) attrs['align']       = style.align;
  if (style?.lineHeight !== undefined) attrs['line-height'] = String(style.lineHeight);
  if (style?.width      !== undefined) attrs['w']           = String(style.width);

  const node: LpdfCanvasTextNode = { type: 'canvas-text', text: content, attrs };
  if (runs && runs.length > 0) {
    node.runs = runs.map(r => {
      const runAttrs: { font?: string; 'font-size'?: string; color?: string } = {};
      if (r.font  !== undefined) runAttrs['font']      = r.font;
      if (r.size  !== undefined) runAttrs['font-size'] = String(r.size);
      if (r.color !== undefined) runAttrs['color']     = r.color;
      return { text: r.text, attrs: runAttrs };
    });
  }
  return node;
}

function img(x: number, y: number, w: number, h: number, name: string): LpdfCanvasImgNode {
  return {
    type:  'canvas-img',
    attrs: { x: String(x), y: String(y), w: String(w), h: String(h), name },
  };
}

function layer(nodes: LpdfCanvasPrimitiveNode[], options?: CanvasLayerOptions): LpdfCanvasLayerNode {
  const attrs: Record<string, string> = {};
  if (options?.page      !== undefined) attrs['page']      = options.page;
  if (options?.opacity   !== undefined) attrs['opacity']   = String(options.opacity);
  if (options?.transform !== undefined) attrs['transform'] = options.transform.toString();
  return { type: 'canvas-layer', attrs, nodes };
}

export const LpdfCanvas = Object.freeze({
  rect,
  line,
  ellipse,
  circle,
  path,
  text: canvasText,
  img,
  layer,
});
