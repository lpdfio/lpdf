/**
 * example-data.ts — render data-invoice.xml with dynamic data from data-invoice.json.
 *
 * Run after building the adapter:
 *   cd src/adapters/node
 *   npm run build
 *   npx ts-node example/example-data.ts
 *
 * Output: example/result/example-data-node.pdf written to the project root.
 */

import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { LpdfEngine } from '../dist/index.js';

(async () => {
  const __root     = resolve(__dirname, '../../../../example/');
  const xmlFile    = resolve(__root, 'xml/data-invoice.xml');
  const jsonFile   = resolve(__root, 'xml/data-invoice.json');
  const outputFile = 'example-data-node.pdf';

  const xml  = readFileSync(xmlFile, 'utf8');
  const data = JSON.parse(readFileSync(jsonFile, 'utf8'));

  const engine = new LpdfEngine('');  // empty key → free tier (watermark)

  const bytes = await engine.renderPdf(xml, { data });

  writeFileSync(resolve(__root, 'result', outputFile), bytes);
  console.log(`output: ${outputFile} (${bytes.length.toLocaleString()} bytes)`);
})();
