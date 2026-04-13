// Bundles src/browser.ts into a self-contained ESM file for use in browsers.
//
// The WASM JS wrapper (dist/web/lpdf.js) and pdf-lib are both bundled in.
// Only the WASM binary (lpdf_bg.wasm) stays external — consumers serve it
// as a static asset and pass its URL to initLpdf().
import { build } from 'esbuild';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const dir = dirname(fileURLToPath(import.meta.url));

await build({
  entryPoints: [resolve(dir, 'src/browser.ts')],
  bundle:      true,
  format:      'esm',
  platform:    'browser',
  outfile:     resolve(dir, 'dist/browser.js'),
  // .wasm files are loaded by the consumer via fetch — do not bundle them.
  loader:      { '.wasm': 'empty' },
});

console.log('browser bundle written to dist/browser.js');
