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
  const __root = resolve(__dirname, '../../../../example/');

  const examples = [
    'example1', 
    'example2',
    'example-contract',
  ];

  // init engine
  const engine = new LpdfEngine('');       // empty key → free tier (watermark)

  // load assets (only used if referenced in xml/layout)
  engine.loadFont('montserrat', readFileSync(resolve(__root, 'assets/fonts/Montserrat-Regular.ttf')));
  engine.loadImage('logo', readFileSync(resolve(__root, 'assets/images/logo-lpdf.png')));

  for (const example of examples) {
    // load xml from file
    const xml = readFileSync(resolve(__root, 'xml', `${example}.xml`), 'utf8');

    // render pdf from xml
    const bytes = await engine.renderPdf(xml);

    // define output file name
    const outputFile = `${example}-node.pdf`;

    // write pdf to file
    writeFileSync(resolve(__root, 'result', outputFile), bytes);

    console.log(`output: ${outputFile} (${bytes.length.toLocaleString()} bytes)`);
  }
})();
