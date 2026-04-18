/**
 * encrypt-open-password.ts — render showcase-encryption.xml with RC4-128 encryption,
 * open password required, copy disabled.
 *
 * Run after building the adapter:
 *   cd src/adapters/node
 *   npm run build
 *   npx ts-node example/encrypt-open-password.ts
 *
 * Output: example/result/encrypt-open-password-node.pdf
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
  const outputFile = 'encrypt-open-password-node.pdf';

  const xml = readFileSync(xmlFile, 'utf8');

  const engine = new WasmEngine('');  // empty key → free tier (watermark)

  // With open password — viewers prompt for 'password' before displaying content.
  engine.set_encryption('password', 'owner', JSON.stringify({ copy: false }));

  const bytes: Uint8Array = engine.render_pdf(xml);
  engine.free();

  writeFileSync(resolve(__root, 'result', outputFile), bytes);
  console.log(`output: ${outputFile} (${bytes.length.toLocaleString()} bytes)`);
})();
