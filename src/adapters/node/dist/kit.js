"use strict";
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
Object.defineProperty(exports, "__esModule", { value: true });
exports.LpdfKit = void 0;
// ── camelCase → kebab-case helper ─────────────────────────────────────────────
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
// ── Factory functions ─────────────────────────────────────────────────────────
function layout(nodes = []) {
    return { type: 'layout', nodes };
}
function canvas(layers = []) {
    return { type: 'canvas', nodes: layers };
}
function section(input = {}) {
    return {
        type: 'section',
        attrs: buildAttrs((input.options ?? {})),
        nodes: input.nodes ?? [],
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
        nodes: input.sections ?? [],
    };
}
// ── LpdfKit export ────────────────────────────────────────────────────────────
exports.LpdfKit = Object.freeze({
    layout,
    canvas,
    section,
    document,
});
