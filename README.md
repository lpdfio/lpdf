<p align="center"><img src="lpdf-light.png" height="48" alt="Lpdf"></p>

# Lpdf

**PDF as Code, on every platform.**

Describe your document structure in code using the programming Kit, or XML. Every PDF is compact, pixel-perfect, and identical across platforms.

---

## Architecture

The core engine is written in Rust and compiled to two targets:

- **WASM** — for Node.js and browser runtimes (embedded in the adapter package)
- **WASI** — a portable `.wasm` binary for server runtimes (Python, PHP, .NET) via [Wasmtime](https://wasmtime.dev)

All adapters expose the same two interfaces:

- **`renderPdf(xml)`** — render a PDF from an XML document string
- **Kit API** — build documents programmatically using `LpdfKit` and `LpdfLayout` without writing XML

The XML schema is published at [`https://lpdf.io/schema/lpdf.xsd`](https://lpdf.io/schema/lpdf.xsd).

---

## Adapters

### Node.js

**[github.com/lpdfio/lpdf-js](https://github.com/lpdfio/lpdf-js)** · [npmjs.com/package/@lpdfio/lpdf](https://www.npmjs.com/package/@lpdfio/lpdf)

```bash
npm install @lpdfio/lpdf
```

```js
const { LpdfEngine } = require('@lpdfio/lpdf');
const fs = require('node:fs');

const engine = new LpdfEngine();
const xml = fs.readFileSync('document.xml', 'utf8');
const pdf = await engine.renderPdf(xml);
fs.writeFileSync('output.pdf', pdf);
```

---

### Python

**[github.com/lpdfio/lpdf-python](https://github.com/lpdfio/lpdf-python)** · [pypi.org/project/lpdfio-lpdf](https://pypi.org/project/lpdfio-lpdf/)

```bash
pip install lpdfio-lpdf
```

```python
from lpdf import LpdfEngine

engine = LpdfEngine()
xml = open("document.xml").read()
pdf = engine.render_pdf(xml)
open("output.pdf", "wb").write(pdf)
```

---

### PHP

**[github.com/lpdfio/lpdf-php](https://github.com/lpdfio/lpdf-php)** · [packagist.org/packages/lpdfio/lpdf](https://packagist.org/packages/lpdfio/lpdf)

```bash
composer require lpdfio/lpdf
```

```php
use Lpdf\LpdfEngine;

$engine = new LpdfEngine();
$xml = file_get_contents('document.xml');
$pdf = $engine->renderPdf($xml);
file_put_contents('output.pdf', $pdf);
```

---

### .NET

**[github.com/lpdfio/lpdf-dotnet](https://github.com/lpdfio/lpdf-dotnet)** · [nuget.org/packages/Lpdfio.Lpdf](https://www.nuget.org/packages/Lpdfio.Lpdf)

```bash
dotnet add package Lpdfio.Lpdf
```

```csharp
using Lpdf;

var engine = new LpdfEngine();
var xml = await File.ReadAllTextAsync("document.xml");
var pdf = await engine.RenderPdf(xml);
await File.WriteAllBytesAsync("output.pdf", pdf);
```

---

## Kit API

All adapters also expose a fluent builder API for constructing documents in code, without XML. Each adapter ships equivalent `LpdfKit` and `LpdfLayout` classes in its native style.

```js
// Node.js — same concepts apply across all adapters
const { LpdfEngine, LpdfKit, LpdfLayout } = require('@lpdfio/lpdf');

const engine = new LpdfEngine();
const doc = LpdfKit.document({
  sections: [
    LpdfKit.section({
      nodes: [
        LpdfKit.layout([
          LpdfLayout.stack([
            LpdfLayout.text(['Invoice #1001'], { fontSize: '24pt', bold: 'true' }),
            LpdfLayout.text(['Due: 2025-06-01']),
          ]),
        ]),
      ],
    }),
  ],
});
const pdf = await engine.renderKit(doc);
```

---

## VS Code Extension

**[marketplace.visualstudio.com](https://marketplace.visualstudio.com/items?itemName=lpdfio.lpdf)**

Preview, design, and export PDFs directly in VS Code — entirely offline. Supports live XML preview and PDF export via the command palette.

---

## Docs

[lpdf.io/docs](https://lpdf.io/docs) · [lpdf.io](https://lpdf.io)


