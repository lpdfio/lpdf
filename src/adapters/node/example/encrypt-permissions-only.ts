/**
 * encrypt-permissions-only.ts — render showcase-encryption.xml with RC4-128 encryption,
 * no open password, print and copy disabled.
 *
 * Run after building the adapter:
 *   cd src/adapters/node
 *   npm run build
 *   npx ts-node example/encrypt-permissions-only.ts
 *
 * Output: example/result/encrypt-permissions-only-node.pdf
 */

import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

// Access the raw WASM engine directly — the high-level LpdfEngine wrapper does not
// yet expose setEncryption, so we go one level down (same module index.ts uses).
// eslint-disable-next-line @typescript-eslint/no-require-imports
const { LpdfEngine: WasmEngine } = require('../../../../dist/node/lpdf.js');

(async () => {
  const __root   = resolve(__dirname, '../../../../example/');
  const xmlFile  = resolve(__dirname, '../../../../docs/examples/showcase-encryption.xml');
  const outputFile = 'encrypt-permissions-only-node.pdf';

  const xml = readFileSync(xmlFile, 'utf8');

  const engine = new WasmEngine('');  // empty key → free tier (watermark)

  // Permissions only — no open password.
  // File opens freely; cooperative viewers enforce print: false, copy: false.
  engine.set_encryption('', 's3cr3t', JSON.stringify({ print: false, copy: false }));

  const bytes: Uint8Array = engine.render_pdf(xml);
  engine.free();

  writeFileSync(resolve(__root, 'result', outputFile), bytes);
  console.log(`output: ${outputFile} (${bytes.length.toLocaleString()} bytes)`);
})();
