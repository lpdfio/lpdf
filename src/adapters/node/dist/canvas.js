"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.LpdfCanvas = exports.CanvasTransform = void 0;
// ── CanvasTransform ───────────────────────────────────────────────────────────
class CanvasTransform {
    constructor(matrix) {
        this.matrix = matrix;
    }
    /** Rotate clockwise by `degrees` around the origin, or around (cx, cy). */
    static rotate(degrees, cx = 0, cy = 0) {
        const rad = (degrees * Math.PI) / 180;
        const cos = Math.cos(rad);
        const sin = Math.sin(rad);
        const e = cx - cx * cos + cy * sin;
        const f = cy - cx * sin - cy * cos;
        return new CanvasTransform([cos, sin, -sin, cos, e, f]);
    }
    /** Scale by sx (and optionally sy; defaults to sx for uniform scale). */
    static scale(sx, sy) {
        return new CanvasTransform([sx, 0, 0, sy ?? sx, 0, 0]);
    }
    /** Translate by (tx, ty). */
    static translate(tx, ty) {
        return new CanvasTransform([1, 0, 0, 1, tx, ty]);
    }
    /**
     * Combine: apply `other` first, then `this`.
     * Equivalent to matrix multiplication: this × other.
     */
    then(other) {
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
    toString() {
        return `matrix(${this.matrix.join(',')})`;
    }
}
exports.CanvasTransform = CanvasTransform;
// ── LpdfCanvas factory ────────────────────────────────────────────────────────
function rect(x, y, w, h, style) {
    const attrs = {
        x: String(x), y: String(y), w: String(w), h: String(h),
    };
    if (style?.fill !== undefined)
        attrs['fill'] = style.fill;
    if (style?.stroke !== undefined)
        attrs['stroke'] = style.stroke;
    if (style?.strokeWidth !== undefined)
        attrs['stroke-width'] = String(style.strokeWidth);
    if (style?.strokeDash !== undefined)
        attrs['stroke-dash'] = style.strokeDash.join(' ');
    if (style?.borderRadius !== undefined)
        attrs['radius'] = String(style.borderRadius);
    return { type: 'canvas-rect', attrs };
}
function line(x1, y1, x2, y2, style) {
    const attrs = {
        x1: String(x1), y1: String(y1), x2: String(x2), y2: String(y2),
    };
    if (style?.stroke !== undefined)
        attrs['stroke'] = style.stroke;
    if (style?.strokeWidth !== undefined)
        attrs['stroke-width'] = String(style.strokeWidth);
    if (style?.strokeDash !== undefined)
        attrs['stroke-dash'] = style.strokeDash.join(' ');
    if (style?.lineCap !== undefined)
        attrs['line-cap'] = style.lineCap;
    if (style?.lineJoin !== undefined)
        attrs['line-join'] = style.lineJoin;
    return { type: 'canvas-line', attrs };
}
function ellipse(cx, cy, rx, ry, style) {
    const attrs = {
        cx: String(cx), cy: String(cy), rx: String(rx), ry: String(ry),
    };
    if (style?.fill !== undefined)
        attrs['fill'] = style.fill;
    if (style?.stroke !== undefined)
        attrs['stroke'] = style.stroke;
    if (style?.strokeWidth !== undefined)
        attrs['stroke-width'] = String(style.strokeWidth);
    if (style?.strokeDash !== undefined)
        attrs['stroke-dash'] = style.strokeDash.join(' ');
    return { type: 'canvas-ellipse', attrs };
}
function circle(cx, cy, r, style) {
    const attrs = {
        cx: String(cx), cy: String(cy), r: String(r),
    };
    if (style?.fill !== undefined)
        attrs['fill'] = style.fill;
    if (style?.stroke !== undefined)
        attrs['stroke'] = style.stroke;
    if (style?.strokeWidth !== undefined)
        attrs['stroke-width'] = String(style.strokeWidth);
    if (style?.strokeDash !== undefined)
        attrs['stroke-dash'] = style.strokeDash.join(' ');
    return { type: 'canvas-circle', attrs };
}
function path(d, style) {
    const attrs = { d };
    if (style?.fill !== undefined)
        attrs['fill'] = style.fill;
    if (style?.stroke !== undefined)
        attrs['stroke'] = style.stroke;
    if (style?.strokeWidth !== undefined)
        attrs['stroke-width'] = String(style.strokeWidth);
    if (style?.strokeDash !== undefined)
        attrs['stroke-dash'] = style.strokeDash.join(' ');
    if (style?.fillRuleEvenodd !== undefined)
        attrs['fill-rule'] = style.fillRuleEvenodd ? 'evenodd' : 'nonzero';
    if (style?.lineCap !== undefined)
        attrs['line-cap'] = style.lineCap;
    if (style?.lineJoin !== undefined)
        attrs['line-join'] = style.lineJoin;
    return { type: 'canvas-path', attrs };
}
function canvasText(x, y, content, style, runs) {
    const attrs = { x: String(x), y: String(y) };
    if (style?.font !== undefined)
        attrs['font'] = style.font;
    if (style?.size !== undefined)
        attrs['font-size'] = String(style.size);
    if (style?.color !== undefined)
        attrs['color'] = style.color;
    if (style?.align !== undefined)
        attrs['align'] = style.align;
    if (style?.lineHeight !== undefined)
        attrs['line-height'] = String(style.lineHeight);
    if (style?.width !== undefined)
        attrs['w'] = String(style.width);
    const node = { type: 'canvas-text', text: content, attrs };
    if (runs && runs.length > 0) {
        node.runs = runs.map(r => {
            const runAttrs = {};
            if (r.font !== undefined)
                runAttrs['font'] = r.font;
            if (r.size !== undefined)
                runAttrs['font-size'] = String(r.size);
            if (r.color !== undefined)
                runAttrs['color'] = r.color;
            return { text: r.text, attrs: runAttrs };
        });
    }
    return node;
}
function img(x, y, w, h, name) {
    return {
        type: 'canvas-img',
        attrs: { x: String(x), y: String(y), w: String(w), h: String(h), name },
    };
}
function layer(nodes, options) {
    const attrs = {};
    if (options?.page !== undefined)
        attrs['page'] = options.page;
    if (options?.opacity !== undefined)
        attrs['opacity'] = String(options.opacity);
    if (options?.transform !== undefined)
        attrs['transform'] = options.transform.toString();
    return { type: 'canvas-layer', attrs, nodes };
}
exports.LpdfCanvas = Object.freeze({
    rect,
    line,
    ellipse,
    circle,
    path,
    text: canvasText,
    img,
    layer,
});
