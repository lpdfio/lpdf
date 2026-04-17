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
        this._disposed = false;
        this._licenseKey = licenseKey;
        this._opts = options;
    }
    /**
     * Register raw TTF/OTF bytes for a custom font name used in `<font src="…">`.
     * Call before `renderPdf`. Returns `this` for chaining.
     */
    loadFont(name, bytes) {
        this._throwIfDisposed();
        this._fonts.set(name, bytes);
        return this;
    }
    /**
     * Release held resources. Idempotent. Subsequent `renderPdf` / `loadFont`
     * calls after disposal will throw.
     */
    dispose() {
        this._disposed = true;
    }
    [Symbol.dispose]() { this.dispose(); }
    _throwIfDisposed() {
        if (this._disposed)
            throw new Error('LpdfEngine has been disposed.');
    }
    async renderPdf(input, callOptions = {}) {
        this._throwIfDisposed();
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
        // Extract glyph advance widths from loaded font bytes and pass them to the
        // engine so the Rust layout pass can measure custom-font text accurately.
        const metrics = {};
        for (const [name, bytes] of allFonts) {
            const w = extractFontWidths(bytes);
            if (w)
                metrics[name] = w;
        }
        if (Object.keys(metrics).length > 0) {
            engine.set_font_metrics(JSON.stringify(metrics));
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
/**
 * Parse the `head`, `cmap` (format 4), and `hmtx` tables from a TrueType /
 * OpenType font binary and extract per-glyph advance widths for the printable
 * ASCII range (code points 32–126), normalised to 1/1000 em units.
 *
 * Returns `null` if the font cannot be parsed (e.g. unsupported cmap format).
 * In that case the Rust layout engine falls back to its built-in AFM tables or
 * the 0.44-em constant — same behaviour as before this feature was added.
 */
function extractFontWidths(bytes) {
    try {
        // Build a DataView over a clean copy of the font bytes.
        const buf = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
        const view = new DataView(buf);
        // ── sfnt table directory ─────────────────────────────────────────────────
        const numTables = view.getUint16(4);
        const tables = {};
        for (let i = 0; i < numTables; i++) {
            const b = 12 + i * 16;
            const tag = String.fromCharCode(view.getUint8(b), view.getUint8(b + 1), view.getUint8(b + 2), view.getUint8(b + 3));
            tables[tag] = view.getUint32(b + 8);
        }
        if (!('head' in tables && 'cmap' in tables && 'hmtx' in tables && 'hhea' in tables)) {
            return null;
        }
        // ── units-per-em (head table, offset 18) ─────────────────────────────────
        const upm = view.getUint16(tables['head'] + 18);
        if (upm === 0)
            return null;
        // ── number of long hMetrics (hhea table, offset 34) ──────────────────────
        const numHMetrics = view.getUint16(tables['hhea'] + 34);
        // ── glyph advance from hmtx ───────────────────────────────────────────────
        // hmtx: numHMetrics × (advanceWidth u16, lsb i16), then lsb-only entries.
        // For glyphs beyond numHMetrics the advance equals the last entry's advance.
        const getAdvance = (glyphId) => {
            const idx = Math.min(glyphId, numHMetrics - 1);
            return view.getUint16(tables['hmtx'] + idx * 4);
        };
        // ── find Unicode BMP cmap subtable ────────────────────────────────────────
        const cmapBase = tables['cmap'];
        const numEncTbls = view.getUint16(cmapBase + 2);
        let subtableOff = -1;
        let bestPriority = 999;
        for (let i = 0; i < numEncTbls; i++) {
            const b = cmapBase + 4 + i * 8;
            const platformId = view.getUint16(b);
            const encodingId = view.getUint16(b + 2);
            const off = cmapBase + view.getUint32(b + 4);
            // Platform 3 enc 1 = Windows Unicode BMP (preferred).
            // Platform 0 (any enc) = Unicode platform (fallback).
            if (platformId === 3 && encodingId === 1 && bestPriority > 0) {
                subtableOff = off;
                bestPriority = 0;
            }
            else if (platformId === 0 && bestPriority > 1) {
                subtableOff = off;
                bestPriority = 1;
            }
        }
        if (subtableOff < 0)
            return null;
        // ── parse cmap format 4 ───────────────────────────────────────────────────
        const fmt = view.getUint16(subtableOff);
        if (fmt !== 4)
            return null;
        const segCount = view.getUint16(subtableOff + 6) >> 1;
        const endCodesOff = subtableOff + 14;
        const startCodesOff = endCodesOff + segCount * 2 + 2; // +2 for reservedPad
        const idDeltaOff = startCodesOff + segCount * 2;
        const idRangeOff = idDeltaOff + segCount * 2;
        const getGlyphId = (cp) => {
            for (let s = 0; s < segCount; s++) {
                const end = view.getUint16(endCodesOff + s * 2);
                if (cp > end)
                    continue;
                const start = view.getUint16(startCodesOff + s * 2);
                if (cp < start)
                    return 0;
                const delta = view.getInt16(idDeltaOff + s * 2);
                const rangeOff = view.getUint16(idRangeOff + s * 2);
                if (rangeOff === 0)
                    return (cp + delta) & 0xffff;
                const glyphOff = idRangeOff + s * 2 + rangeOff + (cp - start) * 2;
                const gid = view.getUint16(glyphOff);
                return gid === 0 ? 0 : (gid + delta) & 0xffff;
            }
            return 0;
        };
        // ── build ASCII width array (code points 32–126) ──────────────────────────
        const ascii = [];
        for (let cp = 32; cp <= 126; cp++) {
            const gid = getGlyphId(cp);
            const raw = gid > 0 ? getAdvance(gid) : 0;
            ascii.push(Math.round(raw * 1000 / upm));
        }
        // Default width = average of non-zero ASCII advances (fallback for non-ASCII).
        const nonZero = ascii.filter(w => w > 0);
        const defWidth = nonZero.length > 0
            ? Math.round(nonZero.reduce((a, b) => a + b, 0) / nonZero.length)
            : 500;
        return { default: defWidth, ascii };
    }
    catch {
        return null;
    }
}
