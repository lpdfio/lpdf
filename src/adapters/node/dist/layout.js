"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.LpdfLayout = void 0;
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
// ── Helper ────────────────────────────────────────────────────────────────────
function makeContainer(type, nodes, options) {
    return { type, attrs: buildAttrs(options ?? {}), nodes };
}
// ── LpdfLayout factory ────────────────────────────────────────────────────────
function stack(nodes = [], options) {
    return makeContainer('stack', nodes, options);
}
function flank(nodes = [], options) {
    return makeContainer('flank', nodes, options);
}
function split(nodes = [], options) {
    return makeContainer('split', nodes, options);
}
function cluster(nodes = [], options) {
    return makeContainer('cluster', nodes, options);
}
function grid(nodes = [], options) {
    return makeContainer('grid', nodes, options);
}
function frame(nodes = [], options) {
    return makeContainer('frame', nodes, options);
}
function link(nodes = [], options) {
    return makeContainer('link', nodes, options);
}
function table(options, nodes = []) {
    return {
        type: 'table',
        attrs: buildAttrs(options),
        nodes,
    };
}
function thead(nodes = [], options) {
    return {
        type: 'thead',
        attrs: buildAttrs((options ?? {})),
        nodes,
    };
}
function tr(nodes = [], options) {
    return {
        type: 'tr',
        attrs: buildAttrs((options ?? {})),
        nodes,
    };
}
function td(nodes = [], options) {
    return {
        type: 'td',
        attrs: buildAttrs((options ?? {})),
        nodes,
    };
}
function text(nodes = [], options) {
    return {
        type: 'text',
        attrs: buildAttrs((options ?? {})),
        nodes,
    };
}
function span(nodes = [], options) {
    return {
        type: 'span',
        attrs: buildAttrs((options ?? {})),
        nodes,
    };
}
function divider(options) {
    return {
        type: 'divider',
        attrs: buildAttrs((options ?? {})),
    };
}
function img(options) {
    return {
        type: 'img',
        attrs: buildAttrs(options),
    };
}
function barcode(options) {
    return {
        type: 'barcode',
        attrs: buildAttrs(options),
    };
}
function region(pin, nodes = [], options) {
    const attrs = { pin };
    if (options?.page !== undefined)
        attrs['page'] = options.page;
    if (options?.w !== undefined)
        attrs['w'] = options.w;
    if (options?.debug !== undefined)
        attrs['debug'] = options.debug;
    return { type: 'layout-region', attrs, nodes };
}
exports.LpdfLayout = Object.freeze({
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
    table,
    thead,
    tr,
    td,
    region,
});
