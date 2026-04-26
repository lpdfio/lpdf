# lpdfio/lpdf

PHP adapter for [lpdf](https://lpdf.io) — pixel-perfect, lightweight, and consistent PDF rendering.

## Installation

```bash
composer require lpdfio/lpdf
```

## Usage

```php
<?php

use Lpdf\LpdfEngine;

$engine = new LpdfEngine('');          // empty key → free tier (watermark)

$engine->loadFont('montserrat', file_get_contents('fonts/Montserrat-Regular.ttf'));
$engine->loadImage('logo', file_get_contents('images/logo.png'));

$xml = file_get_contents('document.xml');
$pdf = $engine->renderPdf($xml);

file_put_contents('output.pdf', $pdf);
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

- PHP 8.2+
- A WASI-capable runtime is bundled — no additional extensions required.

## High-performance FFI (Linux only)

For production Linux deployments, install the optional FFI package for native performance:

```bash
composer require lpdfio/lpdf-ffi
```

## License

Free for individuals, open-source projects, non-profits, and organizations with annual gross revenue under 1,000,000 USD (Community License). A paid license is required for production use by larger organizations.

See [LICENSE](LICENSE) for full terms or visit [lpdf.io/pricing](https://lpdf.io/pricing) to purchase a license.
