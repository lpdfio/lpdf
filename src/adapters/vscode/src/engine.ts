import { Worker } from 'worker_threads';
import * as path from 'node:path';

export class LpdfRenderError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'LpdfRenderError';
  }
}

const RENDER_TIMEOUT_MS = 30_000;

type Resolve = (bytes: Uint8Array) => void;
type Reject  = (err: LpdfRenderError) => void;

let _worker: Worker | undefined;
let _nextId  = 0;
const _pending = new Map<number, { resolve: Resolve; reject: Reject; timer: ReturnType<typeof setTimeout> }>();

function getWorker(): Worker {
  if (_worker) { return _worker; }

  const workerPath = path.join(__dirname, 'worker.js');
  _worker = new Worker(workerPath);

  _worker.on('message', (msg: { id: number; bytes?: Uint8Array; error?: string }) => {
    const entry = _pending.get(msg.id);
    if (!entry) { return; }
    clearTimeout(entry.timer);
    _pending.delete(msg.id);
    if (msg.error !== undefined) {
      entry.reject(new LpdfRenderError(msg.error));
    } else {
      entry.resolve(msg.bytes!);
    }
  });

  _worker.on('error', (err) => {
    const msg = err.message;
    for (const entry of _pending.values()) { clearTimeout(entry.timer); entry.reject(new LpdfRenderError(msg)); }
    _pending.clear();
    _worker = undefined;
  });

  _worker.on('exit', () => {
    for (const entry of _pending.values()) { clearTimeout(entry.timer); entry.reject(new LpdfRenderError('Render worker exited unexpectedly')); }
    _pending.clear();
    _worker = undefined;
  });

  return _worker;
}

export function renderPdf(xml: string, licenseKey: string, xmlDir: string): Promise<Uint8Array> {
  return new Promise<Uint8Array>((resolve, reject) => {
    const id = _nextId = (_nextId + 1) & 0x7fffffff;
    const timer = setTimeout(() => {
      if (_pending.delete(id)) {
        reject(new LpdfRenderError('Render timed out after 30 seconds'));
      }
    }, RENDER_TIMEOUT_MS);
    _pending.set(id, { resolve, reject, timer });
    try {
      getWorker().postMessage({ id, xml, licenseKey, xmlDir });
    } catch (e) {
      clearTimeout(timer);
      _pending.delete(id);
      reject(new LpdfRenderError(e instanceof Error ? e.message : String(e)));
    }
  });
}

export function disposeRenderWorker(): void {
  for (const entry of _pending.values()) { clearTimeout(entry.timer); }
  _pending.clear();
  _worker?.terminate();
  _worker = undefined;
  _nextId = 0;
}
