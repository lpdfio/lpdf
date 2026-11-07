<img src="https://raw.githubusercontent.com/lpdfio/lpdf/main/lpdf-mark.svg" height="48" alt="Lpdf" />

# [Lpdf](https://lpdf.io?utm_campaign=core&utm_medium=referral&utm_source=readme)

**PDF as Code on every platform**

You describe a document as code or XML. Lpdf renders a compact, pixel-perfect PDF — identical across platforms.

## SDKs

Lpdf has SDKs for JavaScript, Python, Java, C#, and PHP. Under the hood, a single Rust core compiles to three targets: **WASM** (embedded in the JS SDK), **WASI** (a portable binary for server runtimes via [Wasmtime](https://wasmtime.dev); the Java SDK carries it compiled to JVM classes and runs it on [Endive](https://github.com/bytecodealliance/endive) instead), and a **native library** for Linux with glibc 2.17 or newer, on x86_64 and arm64, which the PHP SDK loads through FFI where it can, with the WASI binary as its fallback.

### Version Requirements

| Language   | Minimum    |
|------------|------------|
| JavaScript | Node.js 20 |
| Python     | 3.11       |
| Java       | 17         |
| C#         | .NET 8.0   |
| PHP        | 8.2        |

### JavaScript (Node.js 20+)

[github.com/lpdfio/lpdf-js](https://github.com/lpdfio/lpdf-js)  -  [npmjs.com/package/@lpdfio/lpdf](https://www.npmjs.com/package/@lpdfio/lpdf)  -  [lpdf.io/docs](https://lpdf.io/docs/install/?sdk=js&utm_campaign=sdk-node&utm_medium=referral&utm_source=readme)

```bash
npm install @lpdfio/lpdf
```

```js
const { L, NoAttr } = require('@lpdfio/lpdf')

const engine = L.engine()

const doc = L.document({ size: 'letter', margin: '48pt' }, [
    L.section(NoAttr, [
        L.layout(NoAttr, [
            L.column({ gap: '24pt' }, [
                L.row({ justify: 'between' }, [
                    L.text({ fontSize: '8pt', color: '#888888' }, ['ACME CORP']),
                    L.text({ fontSize: '22pt', bold: true }, ['Project Proposal']),
                ]),
                L.divider({ thickness: 'xs' }),
                L.text({ fontSize: '13pt', bold: true }, ['Scope of Work']),
                L.row({ gap: '12pt', align: 'start' }, [
                    L.text({ color: '#888888', width: '24pt' }, ['01']),
                    L.text({ width: 'fill' }, ['Discovery & Research']),
                ]),
            ]),
        ]),
    ]),
])

engine.render(doc).then((pdf) => require('node:fs').writeFileSync('proposal.pdf', pdf))
```

### Python (3.11+)

[github.com/lpdfio/lpdf-python](https://github.com/lpdfio/lpdf-python)  -  [pypi.org/project/lpdfio-lpdf](https://pypi.org/project/lpdfio-lpdf/)  -  [lpdf.io/docs](https://lpdf.io/docs/install/?sdk=python&utm_campaign=sdk-python&utm_medium=referral&utm_source=readme)

```bash
pip install lpdfio-lpdf
```

```python
from lpdf import L, NoAttr, DocumentAttr, ColumnAttr, RowAttr, TextAttr, DividerAttr

engine = L.engine()

doc = L.document(DocumentAttr(size='letter', margin='48pt'), [
    L.section(NoAttr, [
        L.layout(NoAttr, [
            L.column(ColumnAttr(gap='24pt'), [
                L.row(RowAttr(justify='between'), [
                    L.text(TextAttr(font_size='8pt', color='#888888'), ['ACME CORP']),
                    L.text(TextAttr(font_size='22pt', bold=True), ['Project Proposal']),
                ]),
                L.divider(DividerAttr(thickness='xs')),
                L.text(TextAttr(font_size='13pt', bold=True), ['Scope of Work']),
                L.row(RowAttr(gap='12pt', align='start'), [
                    L.text(TextAttr(color='#888888', width='24pt'), ['01']),
                    L.text(TextAttr(width='fill'), ['Discovery & Research']),
                ]),
            ]),
        ]),
    ]),
])

pdf = engine.render(doc)
```

### Java (17+)

[github.com/lpdfio/lpdf-java](https://github.com/lpdfio/lpdf-java)  -  [central.sonatype.com/artifact/io.lpdf/lpdf](https://central.sonatype.com/artifact/io.lpdf/lpdf)  -  [lpdf.io/docs](https://lpdf.io/docs/install/?sdk=java&utm_campaign=sdk-java&utm_medium=referral&utm_source=readme)

```xml
<dependency>
  <groupId>io.lpdf</groupId>
  <artifactId>lpdf</artifactId>
  <version>VERSION</version>
</dependency>
```

```java
import io.lpdf.*;
import java.util.List;
import static io.lpdf.L.NoAttr;

var engine = L.engine();

var doc = L.document(new DocumentAttr().size("letter"), List.of(
    L.section(NoAttr, List.of(
        L.layout(new LayoutAttr().margin("48pt"), List.of(
            L.column(new ColumnAttr().gap("24pt"), List.of(
                L.row(new RowAttr().justify("between"), List.of(
                    L.text(new TextAttr().fontSize("8pt").color("#888888"), List.of("ACME CORP")),
                    L.text(new TextAttr().fontSize("22pt").bold(true), List.of("Project Proposal"))
                )),
                L.divider(new DividerAttr().thickness("xs")),
                L.text(new TextAttr().fontSize("13pt").bold(true), List.of("Scope of Work")),
                L.row(new RowAttr().gap("12pt").align("start"), List.of(
                    L.text(new TextAttr().color("#888888").width("24pt"), List.of("01")),
                    L.text(new TextAttr().width("fill"), List.of("Discovery & Research"))
                ))
            ))
        ))
    ))
));

byte[] pdf = engine.render(doc);
```

### C# (.NET 8.0+)

[github.com/lpdfio/lpdf-dotnet](https://github.com/lpdfio/lpdf-dotnet)  -  [nuget.org/packages/Lpdfio.Lpdf](https://www.nuget.org/packages/Lpdfio.Lpdf)  -  [lpdf.io/docs](https://lpdf.io/docs/install/?sdk=dotnet&utm_campaign=sdk-dotnet&utm_medium=referral&utm_source=readme)

```bash
dotnet add package Lpdfio.Lpdf
```

```csharp
using Lpdf;
using static Lpdf.L;

var engine = L.Engine();

var doc = L.Document(new() { Size = "letter", Margin = "48pt" }, [
    L.Section(NoAttr, [
        L.Layout(NoAttr, [
            L.Column(new() { Gap = "24pt" }, [
                L.Row(new() { Justify = "between" }, [
                    L.Text(new() { FontSize = "8pt", Color = "#888888" }, ["ACME CORP"]),
                    L.Text(new() { FontSize = "22pt", Bold = true }, ["Project Proposal"]),
                ]),
                L.Divider(new() { Thickness = "xs" }),
                L.Text(new() { FontSize = "13pt", Bold = true }, ["Scope of Work"]),
                L.Row(new() { Gap = "12pt", Align = "start" }, [
                    L.Text(new() { Color = "#888888", Width = "24pt" }, ["01"]),
                    L.Text(new() { Width = "fill" }, ["Discovery & Research"]),
                ]),
            ]),
        ]),
    ]),
]);

var pdf = engine.Render(doc);
```

### PHP (8.2+)

[github.com/lpdfio/lpdf-php](https://github.com/lpdfio/lpdf-php)  -  [packagist.org/packages/lpdfio/lpdf](https://packagist.org/packages/lpdfio/lpdf)  -  [lpdf.io/docs](https://lpdf.io/docs/install/?sdk=php&utm_campaign=sdk-php&utm_medium=referral&utm_source=readme)

```bash
composer require lpdfio/lpdf
```

```php
use Lpdf\L;
use Lpdf\Kit\DocumentAttr;
use Lpdf\Layout\ColumnAttr;
use Lpdf\Layout\DividerAttr;
use Lpdf\Layout\RowAttr;
use Lpdf\Layout\TextAttr;
use const Lpdf\NoAttr;

$engine = L::engine();

$doc = L::document(new DocumentAttr(size: 'letter', margin: '48pt'), [
    L::section(NoAttr, [
        L::layout(NoAttr, [
            L::column(new ColumnAttr(gap: '24pt'), [
                L::row(new RowAttr(justify: 'between'), [
                    L::text(new TextAttr(fontSize: '8pt', color: '#888888'), ['ACME CORP']),
                    L::text(new TextAttr(fontSize: '22pt', bold: true), ['Project Proposal']),
                ]),
                L::divider(new DividerAttr(thickness: 'xs')),
                L::text(new TextAttr(fontSize: '13pt', bold: true), ['Scope of Work']),
                L::row(new RowAttr(gap: '12pt', align: 'start'), [
                    L::text(new TextAttr(color: '#888888', width: '24pt'), ['01']),
                    L::text(new TextAttr(width: 'fill'), ['Discovery & Research']),
                ]),
            ]),
        ]),
    ]),
]);

$pdf = $engine->render($doc);
```

## VS Code Extension

**[marketplace.visualstudio.com](https://marketplace.visualstudio.com/items?itemName=lpdfio.lpdf)**  -  **[open-vsx.org](https://open-vsx.org/extension/lpdfio/lpdf)**

Preview, design, and export PDFs directly in VS Code — entirely offline. Supports live XML preview and PDF export via the command palette.

## Docs

[lpdf.io/docs](https://lpdf.io/docs/?utm_campaign=core&utm_medium=referral&utm_source=readme)

## Issues

Report bugs and request features at [github.com/lpdfio/lpdf/issues](https://github.com/lpdfio/lpdf/issues), the one tracker for the engine, the SDKs and the VS Code extension. Questions go to [Discussions](https://github.com/lpdfio/lpdf/discussions). Pull requests are not accepted.

---

Dual-licensed: Community License (free) and Commercial License (paid). See [LICENSE](LICENSE) for full terms.
