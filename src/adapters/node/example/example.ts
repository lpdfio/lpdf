/**
 * example.ts — render invoice.xml from the project root using LpdfEngine.
 *
 * Run after building the adapter:
 *   cd src/adapters/node
 *   npm run build
 *   npx ts-node example/example.ts          (or: node --loader ts-node/esm example/example.ts)
 *
 * Output: example/invoice-node.pdf written to the project root.
 */

import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { LpdfEngine } from '../dist/index.js';

(async () => {
  const __root = resolve(__dirname, '../../../..');

  // init engine
  const engine = new LpdfEngine('');       // empty key → free tier (watermark)

  // optional: load fonts and assets
  
  const inputFile  = 'invoice.xml';
  const outputFile = 'invoice-node.pdf';

  // load xml from file
  const xml = readFileSync(resolve(__root, 'example', inputFile), 'utf8');

  // render pdf from xml
  const bytes  = await engine.renderPdf(xml);

  // write pdf to file
  writeFileSync(resolve(__root, 'example', outputFile), bytes);
  
  console.log(`output: ${outputFile} (${bytes.length.toLocaleString()} bytes)`);
})();
