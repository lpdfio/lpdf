# Lpdf features

What Lpdf officially supports, area by area: the version each feature is in since, where it stops, and its page in the docs. Checked against the engine of v0.35.0.

- **Since** is the first version that has the feature as this file describes it.
- **Refused** means the render stops with an error that says what to change. Lpdf repairs nothing in a document by itself.
- What this file doesn't list is not supported.
- The same document gives the same PDF in every SDK: JavaScript, Python, Java, C# and PHP. Options and methods, such as `createdOn` and `loadImage`, are named here as JavaScript names them, and each SDK has them in its own case.

**Areas:** [PDF standards and output](#pdf-standards-and-output) · [What combines with what](#what-combines-with-what) · [Images](#images) · [Color](#color) · [Fonts and text](#fonts-and-text) · [Layout](#layout) · [Barcodes](#barcodes) · [Forms](#forms) · [Graphics](#graphics) · [Attachments](#attachments) · [Stamping existing PDFs](#stamping-existing-pdfs) · [Data binding](#data-binding) · [Render limits](#render-limits)

## PDF standards and output

A document asks for a standard, for encryption and for linearization in its `<output>` element, and without one of the standards the file is a plain PDF. The column *Written with* says where each of the others is set.

| Supported | Written with | Since | Docs |
|---|---|---|---|
| PDF/A-2b and PDF/A-2u (ISO 19005-2) | `pdfa="2b"`, `pdfa="2u"` | 0.31.0 | [PDF/A](https://lpdf.io/docs/pdfa/) |
| PDF/A-2a, the tagged level | `pdfa="2a"` | 0.33.0 | [PDF/A, level a](https://lpdf.io/docs/pdfa/#level-a) |
| PDF/A-3b, PDF/A-3u and PDF/A-3a (ISO 19005-3), which carry attachments of any kind | `pdfa="3b"`, `pdfa="3u"`, `pdfa="3a"` | 0.33.0 | [PDF/A](https://lpdf.io/docs/pdfa/) |
| Factur-X 1.09.2 and ZUGFeRD 2.5.2 e-invoices, in the profiles MINIMUM, BASIC WL, BASIC, EN 16931, EXTENDED and XRECHNUNG | an attachment named `factur-x.xml` or `xrechnung.xml` in a PDF/A-3 document | 0.33.0 | [Factur-X e-invoices](https://lpdf.io/docs/facturx/) |
| PDF/UA-1 (ISO 14289-1), a tagged PDF for screen readers | `pdfua="1"` | 0.32.0 | [PDF/UA](https://lpdf.io/docs/pdfua/) |
| PDF/X-4 (ISO 15930), for commercial printing, with the press profile as its output intent | `pdfx="4" profile="…"` | 0.32.0 | [PDF/X](https://lpdf.io/docs/pdfx/) |
| Encryption with AES-256, a user and an owner password, and seven permissions | `encrypt="aes-256" allow="…"` | 0.30.0 | [Encryption](https://lpdf.io/docs/encryption/) |
| Encryption with RC4-128, for readers that cannot open AES-256 | `encrypt="rc4-128"` | 0.24.0 | [Encryption](https://lpdf.io/docs/encryption/#cipher) |
| Linearization (ISO 32000-1, Annex F), which readers call fast web view | `linearize="true"` | 0.32.0 | [Fast web view](https://lpdf.io/docs/fast-web-view/) |
| Print-only and screen-only content, as optional content groups | `ocg="print"`, `ocg="screen"` on an element | 0.32.0 | [Print and screen content](https://lpdf.io/docs/print-and-screen/) |
| Bleed, with a TrimBox and a BleedBox on every page | `bleed` on `<document>` or `<section>` | 0.32.0 | [PDF/X, bleed](https://lpdf.io/docs/pdfx/#bleed) |
| Metadata: title, author, subject, keywords and creator | `<meta>` | 0.24.0 | [All elements](https://lpdf.io/docs/xml/elements/#meta) |
| The document's language | `lang` on `<meta>` | 0.32.0 | [PDF/UA](https://lpdf.io/docs/pdfua/#the-attributes) |
| The creation date, as an ISO 8601 date, or date and time | `createdOn` in the render options | 0.31.0 | [PDF/A, the date](https://lpdf.io/docs/pdfa/#the-date) |
| Bookmarks, the outline a viewer shows, in 6 levels | `<bookmark>` | 0.25.0 | [Bookmark](https://lpdf.io/docs/doc/layout/bookmark/) |
| Page labels, so a viewer's page box shows the printed numbers | `page-start`, `page-format`, `page-prefix` on `<section>` | 0.32.0 | [Section, page numbers](https://lpdf.io/docs/doc/section/#page-numbers) |

### PDF/A

- `pdfa` takes `2a`, `2b`, `2u`, `3a`, `3b` and `3u`, and no other value.
- The render needs `createdOn`: the engine has no clock. A render without it, or with one that is not a date, is refused.
- Refused, since a PDF/A file embeds every font: text in a built-in font (`the text "…" is in Helvetica, a font built into PDF readers, which is not embedded`), a declared font whose file was not loaded, a font whose license forbids embedding, and a character its font has no glyph for.
- Refused, since the sRGB output intent covers RGB colors only: a CMYK color or gradient, and a spot color whose `value` is `cmyk()`.
- Refused: a form field, and a barcode's printed digits (`hrt`), both drawn in Helvetica.
- Refused: `encrypt` (`pdfa="2b" and encrypt don't go together`) and `pdfx` on the same `<output>`.
- Refused at part 2: attachments, since a PDF/A-2 file carries only files that are PDF/A themselves. Part 3 carries files of any kind.
- Refused at level a: a `lang` that is not a language tag, and a character of Unicode's private use area.
- Print and screen content shows everywhere: PDF/A has no switching between the two.

Each error is listed under [PDF/A, errors](https://lpdf.io/docs/pdfa/#errors). Lpdf's tests validate a document at each of the six levels with veraPDF on every change.

### Factur-X and ZUGFeRD

- The invoice is a PDF/A-3 document. `factur-x.xml` or `xrechnung.xml` in a document without `pdfa`, or with a level of part 2, is refused.
- Lpdf reads the profile from BT-24 of the XML and writes the Factur-X metadata. It checks nothing else in the XML: the invoice's content is the caller's to check.
- Refused: XML that does not parse or is not UTF-8, a BT-24 that is missing or names none of the six profiles, and `factur-x.xml` beside `xrechnung.xml` in one document.
- `facturx-strict="true"` also refuses an invoice file whose `mime` is not `text/xml`, an invoice `relationship` its profile doesn't allow, and another attachment with no `mime` or of a format the spec doesn't list.
- The legacy ZUGFeRD 2.0 metadata, and document types other than an invoice, are not written.

Each error is listed under [Factur-X, errors](https://lpdf.io/docs/facturx/#errors). Lpdf's tests check an invoice of each profile with the Mustang CLI on every change.

### PDF/UA

- `pdfua` takes `1`, and no other value.
- The tags come from the document's own elements: paragraphs, headings (`role="h1"` to `"h6"`), tables with the header cells of their `<thead>`, lists, links, footnotes, the table of contents, and images, barcodes and layers with their `alt`. Headers, footers, backgrounds and borders are marked as decoration.
- Refused: no `title` or no `lang` on `<meta>`, a `lang` that is not a language tag, an `<img>` or a `<barcode>` of the layout without `alt` outside a header or a footer, a first heading that is not an `h1`, and a heading that skips a level going down.
- Refused, as for PDF/A: text in a built-in font, a font whose file was not loaded or whose license forbids embedding, a character its font has no glyph for, a form field, and a barcode's printed digits.
- Refused: an encrypted file whose `allow` leaves out `accessibility`.
- Refused: bulk records (`a bulk table is not tagged for a reader`).
- The render needs `createdOn`.
- A header in the first column of each row of a table is not marked as one.

Each error is listed under [PDF/UA, errors](https://lpdf.io/docs/pdfua/#errors). Lpdf's tests validate a document with veraPDF's PDF/UA-1 profile on every change.

### PDF/X

- `pdfx` takes `4`, and no other value. PDF/X-1a is not written.
- `pdfx` and `profile` need each other. The profile is an ICC printer profile, in gray, RGB or CMYK, which the printer gives out: Lpdf bundles none. A display profile, such as sRGB, is refused.
- RGB colors and images are tied to sRGB, CMYK colors stay CMYK, and spot colors stay inks of their own. Opacity, shadows and gradients are kept.
- Links print their content and are not clickable, since PDF/X allows no link annotations, with `pdfua` too. Bookmarks stay.
- The file is the print version of print and screen content: screen content stays in the file, off. With `pdfua` as well, everything shows.
- Refused: no `title` on `<meta>`, a render without `createdOn`, text in a built-in font, a font whose file was not loaded or whose license forbids embedding, a character its font has no glyph for, a barcode's printed digits, and a form field.
- Refused: a CMYK color or gradient, or a spot color whose `value` is `cmyk()`, when the press profile is a gray or an RGB one.
- Refused: `encrypt`, `pdfa` and attachments on the same document.

Each error is listed under [PDF/X, errors](https://lpdf.io/docs/pdfx/#errors).

### Encryption

- The document says that it is encrypted, with which cipher and what a reader may do, in its `<output>`, since 0.31.0. The passwords come with each render call.
- `allow` names what a reader may do without the owner password: `print`, `modify`, `copy`, `annotate`, `fill-forms`, `accessibility`, `assemble`, or `all`. What it doesn't name is denied.
- The owner password is required. Refused: a document that asks for encryption rendered without one, passwords for a document that doesn't ask for encryption, and passwords together with `skipEncryption`.
- Refused: `allow` without `encrypt`, a word in `allow` that is not one of the eight, and a cipher that is not `aes-256` or `rc4-128`.
- A password is read as UTF-8 and cut at 127 bytes for AES-256 and at 32 for RC4-128. Lpdf does not prepare it with SASLprep.
- An AES-256 render uses a fresh random key, so the same document gives two different files. RC4-128 gives the same bytes.
- Attachments are encrypted with the rest of the file.

Each error is listed under [Encryption, errors](https://lpdf.io/docs/encryption/#errors).

### Print and screen content

- `ocg` takes `print` or `screen`, on every element that draws. Any other value is refused.
- Hidden content keeps its place in the layout, and stays in the file.
- A link in print content is not clickable.
- Refused: a form field in print content (`a form field can't be ocg="print"`).

## What combines with what

Two settings of `<output>` on one document:

| | PDF/A | PDF/UA | PDF/X | Encryption | Linearization | Attachments |
|---|---|---|---|---|---|---|
| **PDF/A** | | Yes | Refused | Refused | Yes | PDF/A-3: yes. PDF/A-2: refused |
| **PDF/UA** | Yes | | Yes | Yes, when `allow` names `accessibility` | Yes | Yes |
| **PDF/X** | Refused | Yes | | Refused | Yes | Refused |
| **Encryption** | Refused | Yes, when `allow` names `accessibility` | Refused | | Yes: encrypted first, linearized last | Yes: the files are encrypted too |
| **Linearization** | Yes | Yes | Yes | Yes | | Yes |
| **Attachments** | PDF/A-3: yes. PDF/A-2: refused | Yes | Refused | Yes | Yes | |

A Factur-X invoice is a PDF/A-3 file, so it combines as PDF/A-3 does: with PDF/UA and with linearization, and with neither PDF/X nor encryption.

What a document holds, in each kind of file. A file of two standards keeps to both columns: what either refuses is refused. The two rows where PDF/UA and PDF/X differ say which one a file of both follows.

| Content | Plain PDF | PDF/A | PDF/UA | PDF/X |
|---|---|---|---|---|
| Text in an embedded font | Yes | Yes | Yes | Yes |
| Text in a built-in font | Yes | Refused | Refused | Refused |
| Form fields | Yes | Refused | Refused | Refused |
| A barcode's printed digits (`hrt`) | Yes | Refused | Refused | Refused |
| RGB colors: hex, `rgb()`, `hsl()` | Yes | Yes | Yes | Yes, tied to sRGB |
| CMYK colors, and spot colors with a `cmyk()` value | Yes | Refused | Yes | With a CMYK press profile: yes. With a gray or an RGB one: refused |
| Spot colors with an RGB value | Yes | Yes | Yes | Yes |
| Opacity, shadows and gradients | Yes | Yes | Yes | Yes |
| Images of every format, SVG included | Yes | Yes | Yes, each with an `alt` | Yes |
| Links | Clickable | Clickable | Clickable, and tagged | Printed, not clickable, with PDF/UA too: a screen reader reads the link's text as text |
| Bookmarks | Yes | Yes | Yes | Yes |
| Print and screen content (`ocg`) | The viewer switches it | Everything shows | Everything shows, with PDF/X too | The print version: screen content is off |
| Bulk records (`bulk`) | Yes | Levels b and u: yes. Level a: refused | Refused | Yes |

## Images

An image is declared in `<assets>`, and its bytes come from its `src`, which the SDK reads, in Java and C# through the engine's `srcFallback`, or from `loadImage` on the engine. The engine tells the format from the file's bytes. An image is never stretched. Docs: [Image](https://lpdf.io/docs/doc/layout/img/).

| Format | Since | Read | Refused |
|---|---|---|---|
| JPEG | 0.24.0 | A gray or an RGB JPEG, which goes into the PDF as it is. A gray one is written as gray since 0.35.0 | A CMYK JPEG, since 0.35.0 (`the JPEG is in CMYK, which Lpdf doesn't read: save it in RGB`) |
| PNG | 0.24.0 | Gray, RGB and RGBA at 8 or 16 bits, and a palette at 8 bits. The alpha channel of an RGBA PNG is kept, and, since 0.35.0, the transparent entries of a palette | Gray with alpha, fewer than 8 bits, and, since 0.35.0, an interlaced PNG (`the PNG is interlaced, which an image does not read: save it without interlacing`) |
| WebP | 0.35.0 | A still image, with its alpha channel | An animated one |
| GIF | 0.35.0 | A still image, with its transparent color | An animated one |
| TIFF | 0.35.0 | Gray or RGB at 8 or 16 bits a sample, with alpha or without, and gray without alpha at 1, 2 or 4 bits; uncompressed, LZW, Deflate, PackBits or Group 4 fax | Samples of 32 bits, signed or floating-point samples, CMYK, JPEG compression, Group 3 fax, a color palette, gray with alpha that stores white as zero, a plane for each color, and several pages |
| BMP | 0.35.0 | Palettes of 1, 4 and 8 bits, run-length encoded or not, and 16, 24 and 32 bits to a pixel, with alpha when the file has an alpha mask | A BMP that holds a JPEG or a PNG |
| SVG | 0.35.0 | Drawn as vector: see below | See below |
| AVIF, HEIC, JPEG 2000 | | | Refused, with an error that names the format and the ones to use: `AVIF/HEIC images are not supported; use JPEG, PNG, WebP, GIF, TIFF or BMP` |

- The raster formats other than JPEG go into the PDF as their pixels, compressed without loss, and their pixels count toward the render's [memory cap](#render-limits).
- The engine does not fetch URLs or read data URIs.
- Details and each refusal's way out: [Image, formats](https://lpdf.io/docs/doc/layout/img/#formats).

### SVG

- **Drawn:** `path`, `line`, `polyline`, `polygon`, `rect`, `circle` and `ellipse`, a `marker` at each vertex of its path, `svg`, `g`, `defs`, `use` of an element of the same file, `clipPath`, and `transform` on any element.
- **Painting:** `fill`, `stroke`, `stroke-width`, `stroke-linecap`, `stroke-linejoin`, `stroke-miterlimit`, `stroke-dasharray`, `stroke-dashoffset`, `fill-rule`, `opacity`, `fill-opacity` and `stroke-opacity`, as attributes, in a `style` attribute or in a `<style>` block.
- **Colors:** `#rgb`, `#rrggbb`, `rgb()`, a color name, `none`, and `currentColor`, which is the color of the text where the image sits.
- **Refused,** with an error that names the tag: `text`, an `image` inside the SVG, `foreignObject`, a gradient, `mask`, `filter`, `pattern`, `mix-blend-mode`, a `clipPath` whose shapes have different `clip-rule` values or overlap, a `clip-path` on a shape of a `clipPath`, and any reference to another file or a URL.
- **Limits,** each refused with an error that names it: 100,000 elements once `use`, marker and clip path references are followed, 1,000,000 path commands, 100 levels of nesting, 400,000 XML nodes, and entities that expand to 1,000,000 bytes.
- Animation and scripts are left out of the picture.
- Docs: [Image, SVG](https://lpdf.io/docs/doc/layout/img/#svg).

## Color

| Supported | Since | Docs |
|---|---|---|
| Hex colors, and named colors in `<tokens>` | 0.24.0 | [Colors](https://lpdf.io/docs/xml/schema/#colors) |
| `rgb()` and `hsl()`, written to the PDF as RGB | 0.31.0 | [Colors](https://lpdf.io/docs/xml/schema/#colors) |
| `cmyk()`, written to the PDF as CMYK | 0.31.0 | [Colors](https://lpdf.io/docs/xml/schema/#colors) |
| Spot colors, each an ink printed from its own plate, with tints from 0% to 100% and overprint | 0.35.0 | [Spot colors](https://lpdf.io/docs/xml/schema/#spot-colors) |
| Linear and radial gradients, of two colors or of stops | 0.28.0 | [Gradients](https://lpdf.io/docs/xml/schema/#gradients) |
| The sRGB output intent of a PDF/A file | 0.31.0 | [PDF/A](https://lpdf.io/docs/pdfa/#what-the-file-gets) |
| The press profile of a PDF/X file, an ICC printer profile in gray, RGB or CMYK | 0.32.0 | [PDF/X, the press profile](https://lpdf.io/docs/pdfx/#the-press-profile) |

- RGB and CMYK colors can share a document and a page. Lpdf never converts between the two.
- A color has no alpha: `rgba()` is refused. Opacity is set on a layer, a box or a shadow.
- Content that is faded is blended in RGB, its `cmyk()` colors included.
- A color is read strictly. Refused: a malformed color, a value out of its range, a percentage in `rgb()`, `hsl()` without its `%`, commas mixed with spaces, and a color name the tokens don't define.
- A gradient's colors are all CMYK, all RGB, or all tints of one spot color. One that mixes them is refused.
- A gradient fills a `background` or a `fill`. As a text color, a stroke or a border it is refused.
- Only a spot color takes a tint. Refused: a tint of another color, a tint out of its range, a spot color declared twice, two spot colors with one ink, and an ink named `All`, `None`, `Cyan`, `Magenta`, `Yellow` or `Black`.
- Lpdf ships no ink library: a spot color's ink name and the color that stands in for it on screen are the document's.

## Fonts and text

| Supported | Since | Docs |
|---|---|---|
| The 14 standard PDF fonts, built in: Helvetica, Times and Courier in four faces each, Symbol and ZapfDingbats | 0.24.0 | [All elements, font](https://lpdf.io/docs/xml/elements/#font) |
| TrueType fonts (`.ttf`), embedded with only the glyphs the document uses | 0.24.0 | [Installation, images and fonts](https://lpdf.io/docs/install/#images-and-fonts) |
| OpenType fonts with CFF outlines (`.otf`), embedded the same way | 0.31.0 | [Installation, images and fonts](https://lpdf.io/docs/install/#images-and-fonts) |
| Variable fonts, with TrueType or CFF2 outlines, embedded at their default instance | 0.31.0 | [Installation, images and fonts](https://lpdf.io/docs/install/#images-and-fonts) |
| Text aligned left, center or right, and justified | 0.24.0 | [Text](https://lpdf.io/docs/doc/layout/text/) |
| Justified text in an embedded font, and text measured with each character's own width | 0.30.0 | [Text](https://lpdf.io/docs/doc/layout/text/) |
| Spans with their own font, color, underline, strikethrough or link | 0.24.0 | [Text, inline spans](https://lpdf.io/docs/doc/layout/text/#inline-spans-for-mixed-styling) |
| `line-height`, `max-lines` and `ellipsis` | 0.28.0 | [Text, capping a text](https://lpdf.io/docs/doc/layout/text/#capping-a-text) |
| `widows` and `orphans`, from 1 to 10 lines | 0.25.0 | [Text, widows and orphans](https://lpdf.io/docs/doc/layout/text/#widows-and-orphans) |
| Footnotes, numbered by page, section or document | 0.29.0 | [Footnote](https://lpdf.io/docs/doc/layout/footnote/) |
| Code blocks and inline code, with `tab-size` from 1 to 16 | 0.30.0 | [Code](https://lpdf.io/docs/doc/layout/code/) |
| Markdown: CommonMark with tables, strikethrough and footnotes | 0.30.0 | [Markdown](https://lpdf.io/docs/doc/markdown/) |

- A built-in font draws the Windows-1252 characters and `≥`, `≤`, `≠` and `≈`. It draws `?` for most other characters, so text in another script needs an embedded font: see [Markdown, styling](https://lpdf.io/docs/doc/markdown/#styling).
- A built-in font is not embedded, so PDF/A, PDF/UA and PDF/X refuse text in one.
- Lpdf does not hyphenate words: see [Section, columns](https://lpdf.io/docs/doc/section/#columns).
- A word wider than a whole line breaks at the last character that fits.
- A footnote goes in a `<text>` of the layout. In a table, a region or a `<text-at>` it is refused.
- In Markdown, a `$` is text: there is no math. Raw HTML is left out with its words kept, except `<br>` and `<img>`, and an image at a URL is left out. A Markdown nests 31 quotes or 47 list items in one another at most: see [Markdown, how deep it nests](https://lpdf.io/docs/doc/markdown/#how-deep-it-nests).

## Layout

| Supported | Since | Docs |
|---|---|---|
| Page sizes A3, A4, A5, Letter and Legal, a custom width and height, portrait and landscape, set for the document or for each section | 0.24.0 | [XML schema, page sizes](https://lpdf.io/docs/xml/schema/#page-sizes) |
| Automatic pagination, with `paginate`: `no`, `break-before`, `break-after`, `keep-next` | 0.24.0 | [Layout, pagination rules](https://lpdf.io/docs/doc/layout/#pagination-rules) |
| Boxes: `column`, `row`, `grid` of 1 to 12 columns, `frame`, `divider` and `link`, with padding, gaps, backgrounds, borders and rounded corners | 0.26.0 | [Layout](https://lpdf.io/docs/doc/layout/) |
| Tables with `fr`, length and `%` columns, striped rows, and a header row that repeats on every page | 0.24.0 | [Table](https://lpdf.io/docs/doc/layout/table/) |
| `colspan`, `rowspan`, and `keep="with-next"` on a row | 0.29.0 | [Table](https://lpdf.io/docs/doc/layout/table/#spanning-columns) |
| `auto` table columns, `head-border` and `row-border` | 0.30.0 | [Table](https://lpdf.io/docs/doc/layout/table/#column-width-units) |
| Lists with bullets, numbers and checkboxes, nested | 0.30.0 | [List](https://lpdf.io/docs/doc/layout/list/) |
| Headers and footers (`<region pin="top">`, `"bottom"`) on every page, the first, the last, odd or even pages, or a range, with `{page}` and `{pages}` | 0.24.0 | [Layout, pinned regions](https://lpdf.io/docs/doc/layout/#pinned-regions) |
| Running headers, which show the heading of the page they are on | 0.28.0 | [Layout, running headers](https://lpdf.io/docs/doc/layout/#running-headers) |
| Page numbers that count on across sections, in decimal, letters or roman numerals, with a prefix | 0.32.0 | [Section, page numbers](https://lpdf.io/docs/doc/section/#page-numbers) |
| Links within the document, and a table of contents | 0.25.0 | [Table of contents](https://lpdf.io/docs/doc/toc/) |
| Cross-references that print a bookmark's page and label | 0.32.0 | [Bookmark, referring to a bookmark](https://lpdf.io/docs/doc/layout/bookmark/#referring-to-a-bookmark) |
| A section's flow in 1 to 12 columns, with a gap, a rule, balancing, and inset blocks the text goes around | 0.33.0 | [Section, columns](https://lpdf.io/docs/doc/section/#columns) |
| Design tokens: scales for space, text size, border, radius, width and grid, and named colors | 0.24.0 | [XML schema, tokens](https://lpdf.io/docs/xml/schema/#tokens) |
| A `line-height` row in the tokens | 0.28.0 | [XML schema, tokens](https://lpdf.io/docs/xml/schema/#tokens) |
| A `height` row in the tokens, and aliases | 0.29.0 | [XML schema, aliases](https://lpdf.io/docs/xml/schema/#aliases) |
| Lengths in `pt`, `mm` and `in` on every attribute that takes a length | 0.34.0 | [XML schema, units](https://lpdf.io/docs/xml/schema/#units) |

- A length is written with its unit. One in another unit, such as `cm`, `px` or `em`, is refused.
- A document nests 100 levels at most, `<lpdf>` included. A deeper one is refused: `is nested 101 levels deep, past the limit of 100`. Lists nest 47 deep. See [XML schema, document structure](https://lpdf.io/docs/xml/schema/#document-structure).
- A container narrower than 2.5 mm, or with a `height` set lower, holds no other containers.
- Every attribute is checked against the schema. Refused: a value that is not of its type, and an attribute the element doesn't have.
- A section holds any number of layers and at most one `<layout>` or one `<markdown>`. A second one is refused, and so is a `<layout>` directly in `<document>`.
- A table row and a grid row never split across pages. Content taller than a page that can't split is cut at the bottom of the page.
- `break-before`, `break-after` and `keep-next` take effect on elements directly inside `<layout>`.
- In a table, `auto` columns with `fr` columns are refused, and so are a row that spans more columns than the table has, a `rowspan` that runs past the last row, and a `rowspan` on a `<thead>`.
- A document has one table of contents at most. A second one is refused.
- A `<bookmark>` in a layout needs an element after it in the same container. A section takes one as its first child, for its first page, and a layer cannot hold one.
- An inset that reaches below the room the page leaves, and a `col` and `colspan` past the section's `columns`, are refused.

## Barcodes

A barcode is never stretched. Docs: [Barcode](https://lpdf.io/docs/doc/layout/barcode/).

| Type | `type` | Since | Data |
|---|---|---|---|
| QR code, with error correction `L`, `M`, `Q` or `H` | `qr` | 0.24.0 | Any text |
| Code 128 | `code128` | 0.24.0 | Printable ASCII |
| EAN-13 | `ean13` | 0.24.0 | 12 digits, or 13 with the check digit |
| EAN-8 | `ean8` | 0.35.0 | 7 digits, or 8 with the check digit |
| UPC-A | `upca` | 0.35.0 | 11 digits, or 12 with the check digit |
| Code 39, with an optional mod 43 check character | `code39` | 0.35.0 | Digits, the capitals `A` to `Z`, the space and `- . $ / + %` |
| Data Matrix (ECC 200) | `datamatrix` | 0.35.0 | Any text |
| PDF417, with error correction levels 0 to 8 | `pdf417` | 0.35.0 | Any text |
| GS1-128 | `gs1-128` | 0.35.0 | GS1 fields, 48 characters at most |
| GS1 Data Matrix | `gs1-datamatrix` | 0.35.0 | GS1 fields |

- A QR code needs a `height`, or a `width` that is a length, and a Data Matrix and a PDF417 need a `width` or a `height`. Without it they are refused.
- An EAN-13, EAN-8 or UPC-A without its check digit gets it added, and one with it has it checked.
- Data a type can't hold, such as a lowercase letter in a Code 39 or a wrong check digit, draws a gray box with a red border in place of the barcode.
- A PDF417 has 1 to 30 columns and 3 to 90 rows, and holds 928 codewords at most. A Data Matrix has one of the standard's sizes: the squares from 10×10 to 144×144, and the rectangles 8×18, 8×32, 12×26, 12×36, 16×36 and 16×48. A number of `rows` or `columns` its type doesn't have, such as 2 rows of a PDF417 or 11 of a Data Matrix, is refused, and so is a PDF417 whose `rows` times `columns` is over 928.
- GS1 data is checked: an application identifier of 2 to 4 digits before each value, GS1's characters, the length of each field whose identifier has a fixed one, and the check digit of a GTIN, an SSCC and a GLN. See [Barcode, GS1](https://lpdf.io/docs/doc/layout/barcode/#gs1).
- `hrt` prints the data under a linear barcode, in Helvetica, so PDF/A, PDF/UA and PDF/X refuse it.

## Forms

| Supported | Since | Docs |
|---|---|---|
| Fillable AcroForm fields: text, checkbox, dropdown, radio button and button, with a default value, `required`, `readonly`, `max-len` and a button's submit URL | 0.24.0 | [Field](https://lpdf.io/docs/doc/layout/field/) |

- A field's text is drawn in Helvetica.
- PDF/A, PDF/UA and PDF/X refuse a form field: `the form field 'name' can't be in a PDF/X file`.
- A field in print-only content is refused.

## Graphics

| Supported | Since | Docs |
|---|---|---|
| Layers: drawing at exact positions, under or over the flow, on every page or on the pages a scope names, with `opacity` and a `transform` | 0.27.0 | [Layer](https://lpdf.io/docs/doc/layer/) |
| In a layer: `rect`, `circle`, `ellipse`, `line`, `path` with SVG path data, `text-at`, `img-at` and `barcode-at`, placed by coordinates or by one of nine anchor points | 0.27.0 | [Layer, elements](https://lpdf.io/docs/doc/layer/#elements) |
| Shapes in the flow: `rect`, `circle` and `ellipse` | 0.29.0 | [Shape](https://lpdf.io/docs/doc/layout/shape/) |
| `opacity` on a box, which fades the box and everything in it as one | 0.28.0 | [Layout, opacity](https://lpdf.io/docs/doc/layout/#opacity) |
| Drop shadows on boxes and on a layer's shapes | 0.28.0 | [Layout, shadow](https://lpdf.io/docs/doc/layout/#shadow) |
| Gradients as a background or a fill | 0.28.0 | [Gradients](https://lpdf.io/docs/xml/schema/#gradients) |
| Ruled and squared pages: lines by a step or by a count, drawn under everything else on a section's pages | 0.33.0 | [Rulings](https://lpdf.io/docs/xml/schema/#rulings) |
| SVG images, drawn as vector | 0.35.0 | [Image, SVG](https://lpdf.io/docs/doc/layout/img/#svg) |

- A `transform` is `translate`, `scale`, `rotate` or `matrix`, in plain numbers of points.
- A shadow is hard, with no blur.
- In a layer, `opacity` is set on the `<layer>`. On a layer's shape or `-at` element it is refused.
- A layer holds no flow elements and no other layer: a `<text>`, an `<img>` or a `<barcode>` in a layer is refused.
- An `<img-at>` and a `<barcode-at>` need a `width` or a `height`. Without one they are refused.
- A ruling's count goes from 1 to 100, and its smallest cell is 2.5pt. A smaller step, a count that cuts finer, and a step with a count in one direction are refused.

## Attachments

| Supported | Since | Docs |
|---|---|---|
| Files embedded in the PDF, for the whole document, each with a name, a MIME type, a relationship and a description | 0.31.0 | [Attachments](https://lpdf.io/docs/attachments/) |

- A file's bytes come from its `src`, which the SDK reads, in Java and C# through the engine's `srcFallback`, or from `loadFile` on the engine. A file with no bytes is refused.
- A file holds 2 GB at most: a larger one is refused (`more than the 2 GB a PDF can say a file holds`). What its bytes cost of the render's [memory cap](#render-limits) differs by SDK: see [Attachments, size and memory](https://lpdf.io/docs/attachments/#size-and-memory).
- Refused: two files with one name, a file without a name, a `relationship` that is not one of the five, and a `mime` that is not a type and a subtype.
- Refused: attachments in a PDF/A-2 document (`can't be in a PDF/A-2 file`) and in a PDF/X document (`can't be in a PDF/X file`).
- A file pinned to a spot on a page, or tied to one element, is not written.

## Stamping existing PDFs

| Supported | Since | Docs |
|---|---|---|
| `stamp`: PNG images drawn over the pages of a PDF that Lpdf did not make, each in a box on the pages it names | 0.33.0 | [Stamping existing PDFs](https://lpdf.io/docs/stamp/) |

- The image is a PNG, of every color type and bit depth, with the transparency of its alpha channel or of its palette. It is fitted in its box without distortion. Other formats are refused, and so is an interlaced PNG.
- The PDF comes back with an incremental update after its original bytes. A file with an owner password keeps its encryption and permissions, a digital signature stays valid for the revision it signed, a PDF/A file of part 1, 2 or 3 stays PDF/A, and a tagged file stays valid.
- Refused: an input that is not a PDF, a PDF that needs a password to open, a page the PDF doesn't have, a stamp with no `page`, a box with no `x` and `y` and no `anchor`, a `width` or a `height` of 0, and a PNG with transparency on a PDF/A-1 file.
- A stamp covers what is under it and removes nothing: it is not a redaction, and it is not a digital signature.
- A stamped file is no longer linearized.

## Data binding

Data is JSON bound to an XML document, as an option of the render call. Docs: [Data binding](https://lpdf.io/docs/data/binding/).

| Supported | Since | Docs |
|---|---|---|
| `data-value` fills in a text or a barcode's data, `data-source` repeats an element for each item of a list, and `data-if` and `data-if-not` show or hide it | 0.24.0 | [Data binding](https://lpdf.io/docs/data/binding/) |
| `data-value`, `data-if` and `data-if-not` in a layer | 0.27.0 | [Layer, data in a layer](https://lpdf.io/docs/doc/layer/#data-in-a-layer) |
| Attributes from data: `data-href`, `data-name`, `data-color`, `data-background` and `data-fill` | 0.30.0 | [Data binding, attributes from data](https://lpdf.io/docs/data/binding/#attributes-from-data) |
| `$toc`, the entries of the table of contents | 0.25.0 | [Table of contents](https://lpdf.io/docs/doc/toc/) |
| Markdown from the data | 0.30.0 | [Markdown](https://lpdf.io/docs/doc/markdown/#where-the-markdown-comes-from) |
| Bulk records: a table, list, grid or column laid out a page at a time, for hundreds of thousands of records | 0.33.0 | [Bulk records](https://lpdf.io/docs/data/bulk/) |

- Data applies when the document rendered is XML.
- `data-source` on a layer's element is refused.
- An attribute from data takes a string, checked as the attribute is. Refused: a value that is not a string, a color that does not read, and an image the document does not declare. `data-name` picks among the declared images: the data can't bring a new one.
- A bulk container sits directly in the layout and holds plain text. Each rule it sets is refused when broken, with an error that names it: see [Bulk records, the rules](https://lpdf.io/docs/data/bulk/#the-rules). A document with one has no table of contents and no cross-reference, and is not tagged.

## Render limits

| Limit | Default | Set with | Since | Docs |
|---|---|---|---|---|
| Time: a render that runs longer stops with `The render timed out after 5 seconds.` | 5 seconds | `timeout` on the engine's options, in seconds, `0` for none | 0.33.0 | [Installation, time limit](https://lpdf.io/docs/install/#time-limit) |
| Memory: a render that allocates more stops with `the render needs more memory than its cap of 1024 MB` | 1024 MB | `maxMemoryMb` on the engine's options, from 1 to 4096 | 0.33.0 | [Installation, memory limit](https://lpdf.io/docs/install/#memory-limit) |
| Nesting: 100 levels, `<lpdf>` included | 100 | Fixed | 0.33.0 | [XML schema, document structure](https://lpdf.io/docs/xml/schema/#document-structure) |

- There is no limit on the size of the XML, or of a document built in code.
- The engine makes no network calls.
