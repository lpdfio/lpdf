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
 * Render an lpdf XML document to PDF bytes (Node.js).
 * Custom fonts not supplied via `fontBytes` are loaded from disk using the
 * `src` path in the document's `<fonts>` declaration.
 */
export async function renderPdf(
  xml: string,
  options: RenderOptions = {},
): Promise<Uint8Array> {
  const engine = new LpdfEngine(options.licenseKey ?? '');
  const raw = engine.render(xml);
  engine.free();

  const tree = JSON.parse(raw) as RenderTree;
  if (tree.error) throw new Error(tree.error);

  return buildPdf(tree, options.fontBytes ?? {}, (path) => readFileSync(path));
}
