"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.LpdfEngine = exports.kitToXml = exports.LpdfKit = void 0;
const node_fs_1 = require("node:fs");
const _shared_1 = require("./_shared");
// require() path is relative to the compiled output at dist/index.js.
// dist/index.js → ../../../../dist/node/lpdf.js = project-root/dist/node/lpdf.js
// eslint-disable-next-line @typescript-eslint/no-require-imports
const { LpdfEngine: WasmEngine } = require('../../../../dist/node/lpdf.js');
var kit_1 = require("./kit");
Object.defineProperty(exports, "LpdfKit", { enumerable: true, get: function () { return kit_1.LpdfKit; } });
var kit_to_xml_1 = require("./kit-to-xml");
Object.defineProperty(exports, "kitToXml", { enumerable: true, get: function () { return kit_to_xml_1.kitToXml; } });
/**
 * Stateful renderer. Construct once with the license key and optional shared
 * config; call `renderPdf` as many times as needed without repeating the key.
 */
class LpdfEngine {
    constructor(licenseKey, options = {}) {
        this._licenseKey = licenseKey;
        this._opts = options;
    }
    async renderPdf(input, callOptions = {}) {
        const fontBytes = { ...this._opts.fontBytes, ...callOptions.fontBytes };
        const engine = new WasmEngine(this._licenseKey);
        let raw;
        if (typeof input === 'string') {
            raw = engine.render(input);
        }
        else {
            if (!engine.render_tree) {
                throw new Error('renderTree is not supported by the current WASM build — update the lpdf core package.');
            }
            raw = engine.render_tree(JSON.stringify(input));
        }
        engine.free();
        const tree = JSON.parse(raw);
        if (tree.error)
            throw new Error(tree.error);
        return (0, _shared_1.buildPdf)(tree, fontBytes, (path) => (0, node_fs_1.readFileSync)(path));
    }
}
exports.LpdfEngine = LpdfEngine;
