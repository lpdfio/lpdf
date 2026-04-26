# Lpdfio.Lpdf

.NET adapter for [lpdf](https://lpdf.io) — pixel-perfect, lightweight, and consistent PDF rendering.

## Installation

```bash
dotnet add package Lpdfio.Lpdf
```

## Usage

```csharp
using Lpdf;
using Lpdf.Engine;

var engine = new LpdfEngine(
    licenseKey: "",                                       // empty → free tier (watermark)
    options: new EngineOptions { SrcFallback = File.ReadAllBytes });

engine.LoadFont("montserrat", await File.ReadAllBytesAsync("fonts/Montserrat-Regular.ttf"));
engine.LoadImage("logo", await File.ReadAllBytesAsync("images/logo.png"));

var xml   = await File.ReadAllTextAsync("document.xml");
var bytes = await engine.RenderPdf(xml);

await File.WriteAllBytesAsync("output.pdf", bytes);
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

Third-party component licenses are listed in `THIRD_PARTY_LICENSES`.
