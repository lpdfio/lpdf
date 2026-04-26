# lpdfio-lpdf

Python adapter for [lpdf](https://lpdf.io) — pixel-perfect, lightweight, and consistent PDF rendering.

## Installation

```bash
pip install lpdfio-lpdf
```

## Usage

```python
from lpdf import LpdfEngine

engine = LpdfEngine("")          # empty key → free tier (watermark)

engine.load_font("montserrat", open("fonts/Montserrat-Regular.ttf", "rb").read())
engine.load_image("logo", open("images/logo.png", "rb").read())

xml = open("document.xml", "r", encoding="utf-8").read()
pdf = engine.render_pdf(xml)

open("output.pdf", "wb").write(pdf)
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

## Requirements

- Python 3.8+

## License

Free for individuals, open-source projects, non-profits, and organizations with annual gross revenue under 1,000,000 USD (Community License). A paid license is required for production use by larger organizations.

See [LICENSE](LICENSE) for full terms or visit [lpdf.io/pricing](https://lpdf.io/pricing) to purchase a license.
