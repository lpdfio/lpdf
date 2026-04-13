"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Lpdf = void 0;
const node_fs_1 = require("node:fs");
const _shared_1 = require("./_shared");
// require() path is relative to the compiled output at dist/index.js.
// dist/index.js → ../../../../dist/node/lpdf.js = project-root/dist/node/lpdf.js
// eslint-disable-next-line @typescript-eslint/no-require-imports
const { LpdfEngine } = require('../../../../dist/node/lpdf.js');
/**
 * Stateful renderer. Construct once with the license key and optional shared
 * config; call `renderPdf` as many times as needed without repeating the key.
 */
class Lpdf {
    constructor(licenseKey, options = {}) {
        this._licenseKey = licenseKey;
        this._opts = options;
    }
    /**
     * Render an lpdf XML document to PDF bytes (Node.js).
     * Per-call `fontBytes` are merged with the instance-level ones; per-call
     * keys win on collision.  Custom fonts not supplied via `fontBytes` are
     * loaded from disk using the `src` path in the document's `<fonts>`
     * declaration.
     */
    async renderPdf(xml, callOptions = {}) {
        const fontBytes = { ...this._opts.fontBytes, ...callOptions.fontBytes };
        const engine = new LpdfEngine(this._licenseKey);
        const raw = engine.render(xml);
        engine.free();
        const tree = JSON.parse(raw);
        if (tree.error)
            throw new Error(tree.error);
        return (0, _shared_1.buildPdf)(tree, fontBytes, (path) => (0, node_fs_1.readFileSync)(path));
    }
}
exports.Lpdf = Lpdf;
