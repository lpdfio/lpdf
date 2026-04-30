<p align="center"><img src="lpdf-light.png" height="48" alt="Lpdf"></p>

# Lpdf

**PDF as Code on every platform**

You describe a document as code or XML. Lpdf renders a compact, pixel-perfect PDF — identical across platforms.

## SDKs

Lpdf runs on Node.js, browser, .NET, PHP, and Python. Under the hood, a single Rust core compiles to two targets: **WASM** (embedded in the JS SDK) and **WASI** (a portable binary for server runtimes via [Wasmtime](https://wasmtime.dev)).

### Node.js

[github.com/lpdfio/lpdf-js](https://github.com/lpdfio/lpdf-js)  -  [npmjs.com/package/@lpdfio/lpdf](https://www.npmjs.com/package/@lpdfio/lpdf)  -  [lpdf.io/docs/js](https://lpdf.io/docs/js)

```bash
npm install @lpdfio/lpdf
```

```ts
import { L, NoAttr } from 'lpdf'

const engine = L.engine()

const doc = L.document({ size: 'letter', margin: '48pt' }, [
    L.section(NoAttr, [
        L.layout(NoAttr, [
            L.stack({ gap: '24pt' }, [
                L.split(NoAttr, [
                    L.text({ fontSize: '8pt', color: '#888888' }, ['ACME CORP']),
                    L.text({ fontSize: '22pt', bold: 'true' }, ['Project Proposal']),
                ]),
                L.divider({ thickness: 'xs' }),
                L.text({ fontSize: '13pt', bold: 'true' }, ['Scope of Work']),
                L.flank({ gap: '12pt', align: 'start' }, [
                    L.text({ color: '#888888', width: '24pt' }, ['01']),
                    L.text(NoAttr, ['Discovery & Research']),
                ]),
            ]),
        ]),
    ]),
])

const pdf = await engine.render(doc)
```

### .NET

[github.com/lpdfio/lpdf-dotnet](https://github.com/lpdfio/lpdf-dotnet)  -  [nuget.org/packages/Lpdfio.Lpdf](https://www.nuget.org/packages/Lpdfio.Lpdf)  -  [lpdf.io/docs/dotnet](https://lpdf.io/docs/dotnet)

```bash
dotnet add package Lpdfio.Lpdf
```

```csharp
using Lpdf;

var engine = L.Engine();

var doc = L.Document(new() { Size = "letter", Margin = "48pt" }, [
    L.Section(NoAttr, [
        L.Layout(NoAttr, [
            L.Stack(new() { Gap = "24pt" }, [
                L.Split(NoAttr, [
                    L.Text(new() { FontSize = "8pt", Color = "#888888" }, ["ACME CORP"]),
                    L.Text(new() { FontSize = "22pt", Bold = "true" }, ["Project Proposal"]),
                ]),
                L.Divider(new() { Thickness = "xs" }),
                L.Text(new() { FontSize = "13pt", Bold = "true" }, ["Scope of Work"]),
                L.Flank(new() { Gap = "12pt", Align = "start" }, [
                    L.Text(new() { Color = "#888888", Width = "24pt" }, ["01"]),
                    L.Text(NoAttr, ["Discovery & Research"]),
                ]),
            ]),
        ]),
    ]),
]);

var pdf = await engine.Render(doc);
```

### PHP

[github.com/lpdfio/lpdf-php](https://github.com/lpdfio/lpdf-php)  -  [packagist.org/packages/lpdfio/lpdf](https://packagist.org/packages/lpdfio/lpdf)  -  [lpdf.io/docs/php](https://lpdf.io/docs/php)

```bash
composer require lpdfio/lpdf
```

```php
use Lpdf\L;
use const Lpdf\NoAttr;

$engine = L::engine();

$doc = L::document(new DocumentAttr(size: 'letter', margin: '48pt'), [
    L::section(NoAttr, [
        L::layout(NoAttr, [
            L::stack(new StackAttr(gap: '24pt'), [
                L::split(NoAttr, [
                    L::text(new TextAttr(fontSize: '8pt', color: '#888888'), ['ACME CORP']),
                    L::text(new TextAttr(fontSize: '22pt', bold: 'true'), ['Project Proposal']),
                ]),
                L::divider(new DividerAttr(thickness: 'xs')),
                L::text(new TextAttr(fontSize: '13pt', bold: 'true'), ['Scope of Work']),
                L::flank(new FlankAttr(gap: '12pt', align: 'start'), [
                    L::text(new TextAttr(color: '#888888', width: '24pt'), ['01']),
                    L::text(NoAttr, ['Discovery & Research']),
                ]),
            ]),
        ]),
    ]),
]);

$pdf = $engine->render($doc);
```

### Python

[github.com/lpdfio/lpdf-python](https://github.com/lpdfio/lpdf-python)  -  [pypi.org/project/lpdfio-lpdf](https://pypi.org/project/lpdfio-lpdf/)  -  [lpdf.io/docs/python](https://lpdf.io/docs/python)

```bash
pip install lpdfio-lpdf
```

```python
from lpdf import L, NoAttr

engine = L.engine()

doc = L.document(DocumentAttr(size='letter', margin='48pt'), [
    L.section(NoAttr, [
        L.layout(NoAttr, [
            L.stack(StackAttr(gap='24pt'), [
                L.split(NoAttr, [
                    L.text(TextAttr(font_size='8pt', color='#888888'), ['ACME CORP']),
                    L.text(TextAttr(font_size='22pt', bold='true'), ['Project Proposal']),
                ]),
                L.divider(DividerAttr(thickness='xs')),
                L.text(TextAttr(font_size='13pt', bold='true'), ['Scope of Work']),
                L.flank(FlankAttr(gap='12pt', align='start'), [
                    L.text(TextAttr(color='#888888', width='24pt'), ['01']),
                    L.text(NoAttr, ['Discovery & Research']),
                ]),
            ]),
        ]),
    ]),
])

pdf = engine.render(doc)
```

## VS Code Extension

**[marketplace.visualstudio.com](https://marketplace.visualstudio.com/items?itemName=lpdfio.lpdf)**

Preview, design, and export PDFs directly in VS Code — entirely offline. Supports live XML preview and PDF export via the command palette.

## Docs

[lpdf.io/docs](https://lpdf.io/docs)

--

Dual-licensed: Community License (free) and Commercial License (paid). See [LICENSE](LICENSE) for full terms.
