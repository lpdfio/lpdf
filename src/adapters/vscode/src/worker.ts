import { parentPort } from 'worker_threads';
import * as fs from 'node:fs';
import * as path from 'node:path';

interface IWasmEngine {
  render_pdf(xml: string): Uint8Array;
  load_font(name: string, bytes: Uint8Array): void;
  free(): void;
}
interface IWasmModule {
  LpdfEngine: new (licenseKey: string) => IWasmEngine;
}

interface RenderRequest {
  id: number;
  xml: string;
  licenseKey: string;
  xmlDir: string;
}

let _module: IWasmModule | undefined;
function getWasmModule(): IWasmModule {
  if (!_module) {
    const wasmPath = path.join(__dirname, '..', 'wasm', 'lpdf.js');
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    _module = require(wasmPath) as IWasmModule;
  }
  return _module;
}

// Cached engine instance — avoids re-initialising WASM and re-validating the
// license key on every render request.
let _engine: IWasmEngine | undefined;
let _engineKey: string | undefined;

function getEngine(licenseKey: string): IWasmEngine {
  if (_engine && _engineKey === licenseKey) { return _engine; }
  _engine?.free();
  _engine = new (getWasmModule()).LpdfEngine(licenseKey);
  _engineKey = licenseKey;
  return _engine;
}

function extractFontSrcs(xml: string): Map<string, string> {
  const result = new Map<string, string>();
  // Use [\s\S]*? (dotAll equivalent) to match multi-line tags; support both quote styles.
  for (const match of xml.matchAll(/<font\b[\s\S]*?>/g)) {
    const tag  = match[0];
    const name = /\bname=["']([^"']*)["|']/.exec(tag)?.[1];
    const src  = /\bsrc=["']([^"']*)["|']/.exec(tag)?.[1];
    if (name && src) { result.set(name, src); }
  }
  return result;
}

parentPort!.on('message', ({ id, xml, licenseKey, xmlDir }: RenderRequest) => {
  try {
    const engine = getEngine(licenseKey);
    for (const [name, src] of extractFontSrcs(xml)) {
      const fontPath = path.isAbsolute(src) ? src : path.join(xmlDir, src);
      try { engine.load_font(name, fs.readFileSync(fontPath)); } catch { /* fall back to built-in */ }
    }
    const rawBytes = engine.render_pdf(xml);
    // Copy into a new buffer to guarantee we own the ArrayBuffer before transferring.
    // This guards against the WASM engine returning a view into shared WASM memory.
    const bytes = new Uint8Array(rawBytes);
    parentPort!.postMessage({ id, bytes }, [bytes.buffer as ArrayBuffer]);
  } catch (e) {
    // Dispose the cached engine on error to prevent stale state on the next request.
    _engine?.free();
    _engine = undefined;
    _engineKey = undefined;
    parentPort!.postMessage({ id, error: e instanceof Error ? e.message : String(e) });
  }
});
