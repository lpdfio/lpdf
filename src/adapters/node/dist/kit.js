"use strict";
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
Object.defineProperty(exports, "__esModule", { value: true });
exports.LpdfKit = void 0;
// ── Attribute helpers ─────────────────────────────────────────────────────────
/** camelCase → kebab-case for XML attribute names */
function attrKey(camel) {
    return camel.replace(/[A-Z]/g, c => '-' + c.toLowerCase());
}
function buildAttrs(options) {
    const result = {};
    for (const [key, val] of Object.entries(options)) {
        if (val !== undefined) {
            result[attrKey(key)] = val;
        }
    }
    return result;
}
// ── Helper implementations ────────────────────────────────────────────────────
function makeContainer(type, input) {
    return {
        type,
        attrs: buildAttrs(input.options ?? {}),
        children: input.nodes ?? [],
    };
}
function stack(input = {}) {
    return makeContainer('stack', input);
}
function flank(input = {}) {
    return makeContainer('flank', input);
}
function split(input = {}) {
    return makeContainer('split', input);
}
function cluster(input = {}) {
    return makeContainer('cluster', input);
}
function grid(input = {}) {
    return makeContainer('grid', input);
}
function frame(input = {}) {
    return makeContainer('frame', input);
}
function link(input = {}) {
    return makeContainer('link', input);
}
function table(input) {
    return {
        type: 'table',
        attrs: buildAttrs(input.options),
        children: input.nodes ?? [],
    };
}
function thead(input = {}) {
    return {
        type: 'thead',
        attrs: buildAttrs((input.options ?? {})),
        children: input.nodes ?? [],
    };
}
function tr(input = {}) {
    return {
        type: 'tr',
        attrs: buildAttrs((input.options ?? {})),
        children: input.nodes ?? [],
    };
}
function td(input = {}) {
    return {
        type: 'td',
        attrs: buildAttrs((input.options ?? {})),
        children: input.nodes ?? [],
    };
}
function text(input = {}) {
    return {
        type: 'text',
        attrs: buildAttrs((input.options ?? {})),
        children: input.nodes ?? [],
    };
}
function span(input = {}) {
    return {
        type: 'span',
        attrs: buildAttrs((input.options ?? {})),
        children: input.nodes ?? [],
    };
}
function divider(input = {}) {
    return {
        type: 'divider',
        attrs: buildAttrs((input.options ?? {})),
    };
}
function img(input) {
    return {
        type: 'img',
        attrs: buildAttrs(input.options),
    };
}
function barcode(input) {
    return {
        type: 'barcode',
        attrs: buildAttrs(input.options),
    };
}
function page(input = {}) {
    return {
        type: 'page',
        attrs: buildAttrs((input.options ?? {})),
        children: input.nodes ?? [],
    };
}
function document(input = {}) {
    const { tokens, meta, ...restOpts } = input.options ?? {};
    const attrs = {
        ...buildAttrs(restOpts),
    };
    if (tokens !== undefined)
        attrs['tokens'] = tokens;
    if (meta !== undefined)
        attrs['meta'] = meta;
    return {
        version: 1,
        type: 'document',
        attrs,
        children: input.nodes ?? [],
    };
}
// ── LpdfKit export ────────────────────────────────────────────────────────────
/**
 * Static builder kit — plain frozen object, not a class.
 * Import alongside `LpdfEngine`:
 *
 * ```ts
 * import { LpdfEngine, LpdfKit } from 'lpdf';
 * ```
 */
exports.LpdfKit = Object.freeze({
    stack,
    flank,
    split,
    cluster,
    grid,
    frame,
    link,
    text,
    span,
    divider,
    img,
    barcode,
    page,
    document,
    table,
    thead,
    tr,
    td,
});
