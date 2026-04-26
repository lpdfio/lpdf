# @lpdfio/lpdf

Node.js adapter for [lpdf](https://lpdf.io) — pixel-perfect, lightweight, and consistent PDF rendering.

## Installation

```bash
npm install @lpdfio/lpdf
```

## Usage — Node.js

```ts
import { LpdfEngine } from '@lpdfio/lpdf';
import { readFileSync, writeFileSync } from 'node:fs';

const engine = new LpdfEngine('');          // empty key → free tier (watermark)

engine.loadFont('montserrat', readFileSync('fonts/Montserrat-Regular.ttf'));
engine.loadImage('logo', readFileSync('images/logo.png'));

const xml = readFileSync('document.xml', 'utf8');
const pdf = await engine.renderPdf(xml);

writeFileSync('output.pdf', pdf);
```

## Usage — Browser

```ts
import { LpdfEngine } from '@lpdfio/lpdf/browser';

const engine = new LpdfEngine('');

const xml = `<stack><text>Hello, lpdf</text></stack>`;
const pdf = await engine.renderPdf(xml);

const blob = new Blob([pdf], { type: 'application/pdf' });
window.open(URL.createObjectURL(blob));
```

## XML format

Documents are defined in a layout XML format. See the [lpdf documentation](https://lpdf.io/docs) and [examples](https://github.com/lpdfio/lpdf/tree/main/docs/examples) for the full schema.

```xml
<stack spacing="m" padding="l">
  <text font-size="xl" font="Montserrat-Bold">Invoice #1001</text>
  <grid columns="2">
    <text>Date</text>      <text>2026-04-25</text>
    <text>Due</text>       <text>2026-05-25</text>
  </grid>
</stack>
```

## License

Free for individuals, open-source projects, non-profits, and organizations with annual gross revenue under 1,000,000 USD (Community License). A paid license is required for production use by larger organizations.

See [LICENSE](LICENSE) for full terms or visit [lpdf.io/pricing](https://lpdf.io/pricing) to purchase a license.
