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
exports.LpdfRenderError = void 0;
exports.renderPdf = renderPdf;
exports.disposeRenderWorker = disposeRenderWorker;
const worker_threads_1 = require("worker_threads");
const path = __importStar(require("node:path"));
class LpdfRenderError extends Error {
    constructor(message) {
        super(message);
        this.name = 'LpdfRenderError';
    }
}
exports.LpdfRenderError = LpdfRenderError;
const RENDER_TIMEOUT_MS = 30000;
let _worker;
let _nextId = 0;
const _pending = new Map();
function getWorker() {
    if (_worker) {
        return _worker;
    }
    const workerPath = path.join(__dirname, 'worker.js');
    _worker = new worker_threads_1.Worker(workerPath);
    _worker.on('message', (msg) => {
        const entry = _pending.get(msg.id);
        if (!entry) {
            return;
        }
        clearTimeout(entry.timer);
        _pending.delete(msg.id);
        if (msg.error !== undefined) {
            entry.reject(new LpdfRenderError(msg.error));
        }
        else {
            entry.resolve(msg.bytes);
        }
    });
    _worker.on('error', (err) => {
        const msg = err.message;
        for (const entry of _pending.values()) {
            clearTimeout(entry.timer);
            entry.reject(new LpdfRenderError(msg));
        }
        _pending.clear();
        _worker = undefined;
    });
    _worker.on('exit', () => {
        for (const entry of _pending.values()) {
            clearTimeout(entry.timer);
            entry.reject(new LpdfRenderError('Render worker exited unexpectedly'));
        }
        _pending.clear();
        _worker = undefined;
    });
    return _worker;
}
function renderPdf(xml, licenseKey, xmlDir) {
    return new Promise((resolve, reject) => {
        const id = _nextId = (_nextId + 1) & 0x7fffffff;
        const timer = setTimeout(() => {
            if (_pending.delete(id)) {
                reject(new LpdfRenderError('Render timed out after 30 seconds'));
            }
        }, RENDER_TIMEOUT_MS);
        _pending.set(id, { resolve, reject, timer });
        try {
            getWorker().postMessage({ id, xml, licenseKey, xmlDir });
        }
        catch (e) {
            clearTimeout(timer);
            _pending.delete(id);
            reject(new LpdfRenderError(e instanceof Error ? e.message : String(e)));
        }
    });
}
function disposeRenderWorker() {
    for (const entry of _pending.values()) {
        clearTimeout(entry.timer);
    }
    _pending.clear();
    _worker?.terminate();
    _worker = undefined;
    _nextId = 0;
}
//# sourceMappingURL=engine.js.map