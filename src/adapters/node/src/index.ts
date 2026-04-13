import { readFileSync } from 'node:fs';
import { buildPdf, RenderOptions, RenderTree } from './_shared';

// The WASM CJS module is loaded at runtime; we declare only what we use.
interface ILpdfEngine { render(xml: string): string; free(): void; }
interface LpdfEngineConstructor { new(licenseKey: string): ILpdfEngine; }
// require() path is relative to the compiled output at dist/index.js.
// dist/index.js → ../../../../dist/node/lpdf.js = project-root/dist/node/lpdf.js
// eslint-disable-next-line @typescript-eslint/no-require-imports
const { LpdfEngine } = require('../../../../dist/node/lpdf.js') as { LpdfEngine: LpdfEngineConstructor };

export type { RenderOptions } from './_shared';

/**
 * Stateful renderer. Construct once with the license key and optional shared
 * config; call `renderPdf` as many times as needed without repeating the key.
 */
export class Lpdf {
  private readonly _licenseKey: string;
  private readonly _opts: RenderOptions;

  constructor(licenseKey: string, options: RenderOptions = {}) {
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
  async renderPdf(xml: string, callOptions: RenderOptions = {}): Promise<Uint8Array> {
    const fontBytes = { ...this._opts.fontBytes, ...callOptions.fontBytes };
    const engine = new LpdfEngine(this._licenseKey);
    const raw = engine.render(xml);
    engine.free();

    const tree = JSON.parse(raw) as RenderTree;
    if (tree.error) throw new Error(tree.error);

    return buildPdf(tree, fontBytes, (path) => readFileSync(path));
  }
}


