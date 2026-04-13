"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.renderPdf = renderPdf;
const node_fs_1 = require("node:fs");
const _shared_1 = require("./_shared");
// require() path is relative to the compiled output at dist/index.js.
// dist/index.js → ../../../../dist/node/lpdf.js = project-root/dist/node/lpdf.js
// eslint-disable-next-line @typescript-eslint/no-require-imports
const { LpdfEngine } = require('../../../../dist/node/lpdf.js');
/**
 * Render an lpdf XML document to PDF bytes (Node.js).
 * Custom fonts not supplied via `fontBytes` are loaded from disk using the
 * `src` path in the document's `<fonts>` declaration.
 */
async function renderPdf(xml, options = {}) {
    const engine = new LpdfEngine(options.licenseKey ?? '');
    const raw = engine.render(xml);
    engine.free();
    const tree = JSON.parse(raw);
    if (tree.error)
        throw new Error(tree.error);
    return (0, _shared_1.buildPdf)(tree, options.fontBytes ?? {}, (path) => (0, node_fs_1.readFileSync)(path));
}
