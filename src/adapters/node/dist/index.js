"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.LpdfEngine = exports.kitToXml = exports.LpdfKit = void 0;
const node_fs_1 = require("node:fs");
const kit_to_xml_1 = require("./kit-to-xml");
// require() path is relative to the compiled output at dist/index.js.
// dist/index.js → ../../../../dist/node/lpdf.js = project-root/dist/node/lpdf.js
// eslint-disable-next-line @typescript-eslint/no-require-imports
const { LpdfEngine: WasmEngine } = require('../../../../dist/node/lpdf.js');
var kit_1 = require("./kit");
Object.defineProperty(exports, "LpdfKit", { enumerable: true, get: function () { return kit_1.LpdfKit; } });
var kit_to_xml_2 = require("./kit-to-xml");
Object.defineProperty(exports, "kitToXml", { enumerable: true, get: function () { return kit_to_xml_2.kitToXml; } });
/**
 * Stateful renderer. Construct once with the license key and optional shared
 * config; call `renderPdf` as many times as needed without repeating the key.
 */
class LpdfEngine {
    constructor(licenseKey, options = {}) {
        this._fonts = new Map();
        this._licenseKey = licenseKey;
        this._opts = options;
    }
    /**
     * Register raw TTF/OTF bytes for a custom font name used in `<font src="…">`.
     * Call before `renderPdf`. Returns `this` for chaining.
     */
    loadFont(name, bytes) {
        this._fonts.set(name, bytes);
        return this;
    }
    async renderPdf(input, callOptions = {}) {
        // LpdfDocument trees are serialised to XML before being handed to the Rust engine.
        const xml = typeof input === 'string' ? input : (0, kit_to_xml_1.kitToXml)(input);
        // Merge fonts: instance-level loadFont() calls take precedence over the
        // deprecated fontBytes option, which is kept for one-version compat.
        const allFonts = new Map(this._fonts);
        const extraBytes = { ...this._opts.fontBytes, ...callOptions.fontBytes };
        for (const [name, bytes] of Object.entries(extraBytes)) {
            if (!allFonts.has(name))
                allFonts.set(name, bytes);
        }
        // Auto-load fonts declared via <font src="…"> that haven't been
        // explicitly provided — mirrors the old srcFallback behaviour.
        for (const [name, src] of extractFontSrcs(xml)) {
            if (!allFonts.has(name)) {
                try {
                    allFonts.set(name, (0, node_fs_1.readFileSync)(src));
                }
                catch { /* not found; Rust falls back to Helvetica */ }
            }
        }
        const engine = new WasmEngine(this._licenseKey);
        for (const [name, bytes] of allFonts) {
            engine.load_font(name, bytes);
        }
        const pdf = engine.render_pdf(xml);
        engine.free();
        return pdf;
    }
}
exports.LpdfEngine = LpdfEngine;
// ── Helpers ───────────────────────────────────────────────────────────────────
/** Extract `name → src` pairs from `<font name="…" src="…">` tags in XML. */
function extractFontSrcs(xml) {
    const result = new Map();
    for (const match of xml.matchAll(/<font\s[^>]*>/g)) {
        const tag = match[0];
        const name = /\bname="([^"]*)"/.exec(tag)?.[1];
        const src = /\bsrc="([^"]*)"/.exec(tag)?.[1];
        if (name && src)
            result.set(name, src);
    }
    return result;
}
