# Changelog

## v0.24.0 — 2026-10-05
The first public release of Lpdf.

Lpdf turns an XML document, or the same structure built in code, into a PDF. The engine is written in Rust and runs inside your own process: it makes no network calls, and the same input gives the same PDF in every SDK.

Lpdf is at 0.x, so the XML format and the APIs can still change between minor versions. When they do, breaking changes come first in these notes.

### Documents
- An XML format with a published schema (`lpdf.xsd`), for validation and editor autocomplete
- Sections, each with its own page size (A3, A4, A5, Letter, Legal or custom), orientation, margins and background
- Automatic pagination
- Headers and footers at the top or bottom of the page: on every page, the first, the last, odd or even pages, or a page range, with `{page}` and `{pages}` for page numbers
- Document metadata: title, author, subject, keywords and creator

### Layout
- Layout elements: `stack` (vertical), `flank` (one side fills, the other fits), `split` (proportional columns), `cluster` (wrapping row), `grid` (up to 12 columns) and `frame` (a centred box)
- Padding, gaps, backgrounds, borders and rounded corners on every layout element
- Text aligned left, centre, right or justified, with inline runs that set their own font, colour, underline, strikethrough or link
- Tables with `fr`, `pt` and `%` columns, striped rows, and a header row that repeats on every page
- Images: JPEG and PNG, including transparency
- Dividers, and links around any content
- Barcodes: QR, Code 128 and EAN-13
- Fillable form fields: text, checkbox, dropdown, radio button and button

### Canvas
- Drawing at exact positions, above or below the layout: rectangles, circles, ellipses, lines, paths, text and images
- Positions by coordinates, or by one of nine anchor points plus an offset
- Layers per page or page range, with opacity and transforms

### Styling
- Design tokens: define spacing, borders, radii, widths, text sizes and colours once, then use their names everywhere
- The 14 standard PDF fonts built in, plus custom TrueType and OpenType fonts, embedded with only the glyphs the document uses

### Data
- Bind JSON data to a document: `data-value` fills in text, `data-source` repeats an element for each item in a list, and `data-if` / `data-if-not` show or hide it

### Security
- Password protection and permissions (printing, copying, editing, form filling and more), set on the engine in every SDK, with RC4-128 encryption

Versions before v0.24.0 were pre-release builds used to set up publishing, and have no release notes.
