"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.LpdfEngine = exports.LpdfRenderError = void 0;
const node_fs_1 = require("node:fs");
// require() path is relative to the compiled output at dist/engine.js.
// In the published package, wasm-pack artifacts live in dist/wasm/.
// In the monorepo, `npm run build` copies them there from <root>/dist/node/.
// eslint-disable-next-line @typescript-eslint/no-require-imports
const wasmModule = require('./wasm/lpdf.js');
const WasmEngine = wasmModule.LpdfEngine;
/** Thrown when the lpdf engine returns a layout or parse error. */
class LpdfRenderError extends Error {
    constructor(message) {
        super(message);
        this.name = 'LpdfRenderError';
    }
}
exports.LpdfRenderError = LpdfRenderError;
class LpdfEngine {
    constructor(licenseKey, options = {}) {
        this._fonts = new Map();
        this._images = new Map();
        this._disposed = false;
        this._encrypt = null;
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
     * Register raw image bytes (PNG or JPEG) for an image name used in `<img name="…">`.
     * Call before `renderPdf`. Returns `this` for chaining.
     */
    loadImage(name, bytes) {
        this._throwIfDisposed();
        this._images.set(name, bytes);
        return this;
    }
    /**
     * Configure RC4-128 encryption for all subsequent `renderPdf` calls.
     * Returns `this` for chaining.
     */
    setEncryption(options) {
        this._throwIfDisposed();
        this._encrypt = options;
        return this;
    }
    /**
     * Remove any previously configured encryption.
     * Returns `this` for chaining.
     */
    clearEncryption() {
        this._throwIfDisposed();
        this._encrypt = null;
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
        // Merge fonts: instance-level loadFont() calls take precedence over the
        // deprecated fontBytes option, which is kept for one-version compat.
        const allFonts = new Map(this._fonts);
        const extraBytes = { ...this._opts.fontBytes, ...callOptions.fontBytes };
        for (const [name, bytes] of Object.entries(extraBytes)) {
            if (!allFonts.has(name))
                allFonts.set(name, bytes);
        }
        const engine = new WasmEngine(this._licenseKey);
        const createdOn = callOptions.createdOn ?? this._opts.createdOn;
        if (createdOn) {
            engine.set_created_on(createdOn);
        }
        let pdf;
        try {
            if (typeof input === 'string') {
                // XML path — auto-load fonts and images declared via src="…".
                const xml = input;
                for (const [key, src] of extractAssetSrcs(xml, 'font')) {
                    if (!allFonts.has(key)) {
                        try {
                            allFonts.set(key, (0, node_fs_1.readFileSync)(src));
                        }
                        catch { /* not found; Rust falls back to Helvetica */ }
                    }
                }
                for (const [name, bytes] of allFonts) {
                    engine.load_font(name, bytes);
                }
                for (const [name, bytes] of this._images) {
                    engine.load_image(name, bytes);
                }
                // Auto-load images declared via <image src="…"> that weren't pre-loaded.
                for (const [key, src] of extractAssetSrcs(xml, 'image')) {
                    if (!this._images.has(key)) {
                        try {
                            engine.load_image(key, (0, node_fs_1.readFileSync)(src));
                        }
                        catch { /* skip unresolvable image */ }
                    }
                }
                if (this._encrypt) {
                    const permsJson = JSON.stringify(this._encrypt.permissions ?? {});
                    engine.set_encryption(this._encrypt.userPassword, this._encrypt.ownerPassword, permsJson);
                }
                const dataJson = callOptions.data != null ? JSON.stringify(callOptions.data) : null;
                pdf = engine.render_pdf(xml, dataJson);
            }
            else {
                // JSON (Kit tree) path — pass JSON directly to render_tree_pdf.
                const json = JSON.stringify(input);
                for (const [key, src] of extractFontSrcsFromJson(json)) {
                    if (!allFonts.has(key)) {
                        try {
                            allFonts.set(key, (0, node_fs_1.readFileSync)(src));
                        }
                        catch { /* not found; Rust falls back to Helvetica */ }
                    }
                }
                for (const [name, bytes] of allFonts) {
                    engine.load_font(name, bytes);
                }
                for (const [name, bytes] of this._images) {
                    engine.load_image(name, bytes);
                }
                if (this._encrypt) {
                    const permsJson = JSON.stringify(this._encrypt.permissions ?? {});
                    engine.set_encryption(this._encrypt.userPassword, this._encrypt.ownerPassword, permsJson);
                }
                pdf = engine.render_tree_pdf(json);
            }
        }
        catch (e) {
            engine.free();
            const msg = e instanceof Error ? e.message : String(e);
            throw new LpdfRenderError(msg);
        }
        engine.free();
        return pdf;
    }
}
exports.LpdfEngine = LpdfEngine;
// ── Helpers ───────────────────────────────────────────────────────────────────
/** Extract `ref??name → src` pairs from `<font>` or `<image>` tags in XML. */
function extractAssetSrcs(xml, tag) {
    const result = new Map();
    const re = tag === 'font' ? /<font\s[^>]*>/g : /<image\s[^>]*>/g;
    for (const match of xml.matchAll(re)) {
        const t = match[0];
        const name = /\bname="([^"]*)"/.exec(t)?.[1];
        const ref = /\bref="([^"]*)"/.exec(t)?.[1];
        const src = /\bsrc="([^"]*)"/.exec(t)?.[1];
        const key = ref ?? name;
        if (key && src)
            result.set(key, src);
    }
    return result;
}
/** Extract `ref??name → src` pairs from `attrs.tokens.fonts[name].src` in a kit JSON string. */
function extractFontSrcsFromJson(json) {
    const result = new Map();
    try {
        const doc = JSON.parse(json);
        const fonts = doc?.attrs?.tokens?.fonts ?? {};
        for (const [name, def] of Object.entries(fonts)) {
            if (def.src) {
                const key = def.ref ?? name;
                result.set(key, def.src);
            }
        }
    }
    catch { /* ignore */ }
    return result;
}
