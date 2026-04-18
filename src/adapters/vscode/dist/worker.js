"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
const worker_threads_1 = require("worker_threads");
const fs = __importStar(require("node:fs"));
const path = __importStar(require("node:path"));
let _module;
function getWasmModule() {
    if (!_module) {
        const wasmPath = path.join(__dirname, '..', 'wasm', 'lpdf.js');
        // eslint-disable-next-line @typescript-eslint/no-require-imports
        _module = require(wasmPath);
    }
    return _module;
}
// Cached engine instance — avoids re-initialising WASM and re-validating the
// license key on every render request.
let _engine;
let _engineKey;
function getEngine(licenseKey) {
    if (_engine && _engineKey === licenseKey) {
        return _engine;
    }
    _engine?.free();
    _engine = new (getWasmModule()).LpdfEngine(licenseKey);
    _engineKey = licenseKey;
    return _engine;
}
function extractFontSrcs(xml) {
    const result = new Map();
    // Use [\s\S]*? (dotAll equivalent) to match multi-line tags; support both quote styles.
    for (const match of xml.matchAll(/<font\b[\s\S]*?>/g)) {
        const tag = match[0];
        const name = /\bname=["']([^"']*)["|']/.exec(tag)?.[1];
        const src = /\bsrc=["']([^"']*)["|']/.exec(tag)?.[1];
        if (name && src) {
            result.set(name, src);
        }
    }
    return result;
}
worker_threads_1.parentPort.on('message', ({ id, xml, licenseKey, xmlDir }) => {
    try {
        const engine = getEngine(licenseKey);
        for (const [name, src] of extractFontSrcs(xml)) {
            const fontPath = path.isAbsolute(src) ? src : path.join(xmlDir, src);
            try {
                engine.load_font(name, fs.readFileSync(fontPath));
            }
            catch { /* fall back to built-in */ }
        }
        const rawBytes = engine.render_pdf(xml);
        // Copy into a new buffer to guarantee we own the ArrayBuffer before transferring.
        // This guards against the WASM engine returning a view into shared WASM memory.
        const bytes = new Uint8Array(rawBytes);
        worker_threads_1.parentPort.postMessage({ id, bytes }, [bytes.buffer]);
    }
    catch (e) {
        // Dispose the cached engine on error to prevent stale state on the next request.
        _engine?.free();
        _engine = undefined;
        _engineKey = undefined;
        worker_threads_1.parentPort.postMessage({ id, error: e instanceof Error ? e.message : String(e) });
    }
});
//# sourceMappingURL=worker.js.map