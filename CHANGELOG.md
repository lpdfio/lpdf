# Changelog

## v0.28.0 — 2026-10-19
### Text
- `max-lines` and `ellipsis` work on a `<text>` in the layout, as they do on `<text-at>`. `max-lines="2"` draws at most two lines of the wrapped text, whole lines only; the lines it drops take no room, and a text that splits across pages keeps the cap across them. With `ellipsis="true"` the last line drawn ends in dots. Without `max-lines` a text wraps as before
- `line-height` works on a `<text>` in the layout: a number, as a factor of the font size, or a name from the new `line-height` row of the tokens. The default is 1.2, which is what flow text always used, so a text without it looks the same. A text splits across pages by its own line height. `<text-at>` takes the token names too
- `color` on a `<column>`, `<row>`, `<grid>`, `<frame>` or `<td>` is the color of the text inside it. A text takes the first `color` it finds walking up its boxes, and the `text` color when none sets one. A text or a span with a color of its own keeps it
- `<layer>` takes `font`, `font-size` and `color`: the defaults of the `<text-at>` in it, which keeps what it sets itself

### Boxes and fills
- `opacity` on a `<column>`, `<row>`, `<grid>`, `<frame>` or `<td>` fades the box and everything in it as one, so children that overlap don't show through each other. Boxes nest: a box at 0.5 inside one at 0.5 draws at 25%. At 0 it draws nothing. A box that splits across pages is faded on each page
- `shadow="2pt 3pt #000000 0.15"` draws a drop shadow under a box, or under a layer's `<rect>`, `<circle>` or `<ellipse>`: the offset across and down, a color, and an optional opacity from 0 to 1. It is a hard shadow, with no blur, in the shape of the element, rounded corners included. A box with no background is see-through, so its shadow shows only outside it. The shadow takes no room in the layout
- Gradients. A `<gradients>` block in the tokens defines linear and radial gradients by name: two colors with `from` and `to`, or any number of `<stop offset="…" color="…"/>`, and an `angle` for a linear one (0° runs top to bottom, 90° left to right). A gradient is a fill: `background="gradient:name"` on a box, a table, a `<thead>`, a `<tr>`, a section or the document, and `fill="gradient:name"` on a layer's `<rect>`, `<circle>`, `<ellipse>` and `<path>`. It spans the element it fills, and a box that splits across pages has it on each page. A gradient as a text `color`, a `stroke` or a border is an error that says so

### Images
- **An `<img>` with `padding`, `background`, `border` or `radius` is drawn as a box around its picture.** They were accepted and ignored until now, so an image that set one looks different. `width` and `height` size the picture, and the box is larger by the padding. The border is drawn on the edge of the box and takes no room. `radius` rounds the box and clips the picture to rounded corners, which follow the inside of the box. With no box, `radius` alone clips the picture. With none of the four, an image is drawn as before
- `width="fill"` on an `<img>` makes it as wide as the room it has, in a column, a row (shared with other `fill` children) or a table cell, and its height follows its proportions. A `height` limits it. It stayed at its natural size until now

### Page headers and regions
- Running headers. A `<text capture="chapter">` in the flow remembers its text, and a `<text capture-ref="chapter"/>` in a `<region>` shows the value for the page it is on: the new value on the page where the heading appears, the latest earlier value on a page with none, and `capture-fallback` before any. `capture-mode="first"` or `"last"` (the default) picks between several on one page, so a dictionary's header can read "Abacus – Adrift". A header always keeps room for its lines, one or `max-lines`. A table header that repeats on each page it continues on captures once, on the page it starts
- A `<region>` is drawn at the edge its `pin` names, wherever it is written. **A `<region pin="bottom">` written before the flow was drawn at the top**; it is drawn at the bottom now, so a document that wrote its footer first gets its footer where it asked. Regions on one edge stack in the order written

### Code generation
- The code generated from an XML document, in the VS Code command and in the code tabs of lpdf.io, keeps the space beside a span: `['See the ', L.span(…), ' for details.']`. It trimmed the strings, so the code read "See thedocumentationfor details." and no longer rendered the PDF of its XML

### SDKs
- The attribute classes of all four SDKs carry the new attributes: `max-lines`, `ellipsis` and `line-height` on text, `color`, `opacity` and `shadow` on boxes, `font`, `font-size` and `color` on layers, `capture`, `capture-ref`, `capture-mode` and `capture-fallback` on text. `background` and `fill` take `gradient:name`
- The tokens take `gradients`, a map from a gradient's name to its `type`, `from`, `to` and `angle`, with its `stops` as a list of `{ offset, color }`, and a `line-height` row

### VS Code extension
- Hover, autocomplete and validation follow the schema, so they cover the new attributes and token rows
- The preview no longer asks where to save a PDF when another file is selected after a document with form fields, such as one with `<field>` elements

## v0.27.0 — 2026-10-18
### Layers
- **Breaking:** `<canvas>` is removed, and layers go directly in the section. Layers before the section's `<layout>` paint under it, and layers after it paint over it, so one section can draw both under and over its flow. A document that uses `<canvas>`, or `"type": "canvas"` in a JSON tree, fails with an error that says its layers go directly in the section
- **Breaking:** a section takes at most one `<layout>`. A second one was joined to the first, and now fails with an error
- **Breaking:** text and images in a layer are `<text-at>` and `<img-at>`, and `<text>` or `<img>` in a layer fails with an error that names the new element. They are the layout's own text and images, placed at `x` and `y`: a `<text-at>` draws its spans and fills in `{page}` and `{pages}`, which count within the section
- **Breaking:** `w` and `h` are now `width` and `height` on `<rect>`, `<text-at>` and `<img-at>`. Circles and ellipses lose `cx` and `cy`: `x` and `y` place their box, as for a `<rect>`, and `anchor` attaches the box's matching point. Each old attribute fails with an error that names its replacement
- **Breaking:** `opacity` is set on `<layer>` only. On a shape or text in a layer it is an error: to fade one element, put it in a layer of its own. A shape's own opacity was ignored until now
- Text in a layer has the same defaults as the layout's: the document's font or Helvetica, 11pt and the `text` colour (`#1a1a1a` unless the document redefines it). It was Helvetica, 12pt and black whatever the document set, so layer text without a font, size or colour of its own now looks different
- `<text-at>` is one line by default. With `width`, a longer line is cut off at the width, or ends in an ellipsis with `ellipsis="true"`, and `max-lines` wraps the text within the width, drawing whole lines only. `line-height` sets the distance between lines, 1.2 times the font size by default
- `<barcode-at>` places a barcode in a layer
- `data-if` and `data-if-not` work on `<layer>`, `<text-at>`, `<img-at>` and `<barcode-at>`, and `data-value` fills in a `<text-at>` or a `<barcode-at>`
- A layer with an `opacity` below 1 is faded as one: shapes that overlap inside it no longer show through each other
- A layer at `opacity="0"` draws nothing. It was drawn at full strength

### Layout
- **Breaking:** `margin` moves from `<document>` and `<section>` to `<layout>`, since only the flow uses it: layers measure from the page edges. It now defaults to 36pt, where it was 0, so a layout without a margin gets one: set `margin="0"` on it to keep the old pages. `margin` on a document or a section fails with an error that says it goes on `<layout>`
- **Breaking:** barcodes lose `size`, in the flow and in layers: a QR code takes `width` or `height`. A barcode's printed digits (`hrt`) now sit inside its height, where they added 12pt below it: add 12pt to `height` to keep the bars as tall. `size` fails with an error that names its replacement
- Images and barcodes keep their proportions, in the flow and in layers. Given `width` or `height`, the other follows: an image's own proportions, a square for a QR code, the standard symbol's for an EAN-13, and the bars at a default bar width for a Code 128. Given both, they are a maximum: the image or barcode takes the largest size that fits, and its box becomes that size, so `anchor` in a layer and the parent's `align` in the flow place the image itself. One that was stretched to both now comes out smaller in one direction, which can move what follows it. For slots of the same size, such as a row of logos, put each in a `<frame>` of that size

### SDKs
- **Breaking:** `L.canvas` is removed, and `L.section` takes layers and one `L.layout`. A layer's text, images and barcodes are `textAt`, `imgAt` and `barcodeAt` (`TextAt`, `ImgAt` and `BarcodeAt` in .NET, `text_at`, `img_at` and `barcode_at` in Python), with `TextAtAttr`, `ImgAtAttr` and `BarcodeAtAttr`
- **Breaking:** `margin` leaves `DocumentAttr` and `SectionAttr`, and `L.layout` takes it in a `LayoutAttr`
- **Breaking:** the attribute classes follow the schema: `width` and `height` in place of `w` and `h`, no `cx` or `cy` on circles and ellipses, no `size` on barcodes, and `opacity` on layers only
- **Breaking:** the strings and spans of a text are read as XML reads them, so a space falls between two of them only where a string has one: `["Total: ", L.span(…)]` reads "Total: 5", and `["Total:", L.span(…)]` now reads "Total:5". A text and the XML written from it render the same PDF
- A document built with the `L` builder uses the document's `font`, as its XML does. It was always Helvetica

### VS Code extension
- Hover, autocomplete and validation cover layers in the section, `text-at`, `img-at` and `barcode-at`, and `margin` on the layout, and the document templates use them

## v0.26.0 — 2026-10-16
### Layout
- **Breaking:** `<stack>` is now `<column>`, and `<cluster>` is now `<row>`, so each name says which way it lays out. Both work exactly as before, with the same attributes. A document that uses an old name fails with an error that names the new one, such as `<stack> at line 4, col 9: renamed to <column>`, and so does `"type": "stack"` in a JSON tree
- **Breaking:** `<flank>` and `<split>` are removed, with their `end` and `equal` attributes, since a row now does what each did. `<flank>` becomes a `<row>` whose last child has `width="fill"` (its first child, for `end="true"`). `<split>` becomes `<row justify="between">`, and `<split equal="true">` a `<row>` with `width="fill"` on both children. A converted document renders the same PDF, except where a split's second child was wider than the space left: a row moves it to the next line, so give it `width="fill"`. A document that still uses either element fails with an error that says what to write instead
- `width="fill"` on a child of a row takes the width its line leaves after the other children, shared equally when several on the line use it. A fill child never moves to the next line. In a column it takes the full width
- `justify="between"` on a row puts each line's first child at the left edge, its last at the right, and the others evenly between

### SDKs
- **Breaking:** the `L` builder's `stack` and `cluster` are now `column` and `row`, with `ColumnAttr` and `RowAttr` in place of `StackAttr` and `ClusterAttr`. `flank` and `split` are removed, with `FlankAttr` and `SplitAttr`: use `row` with `width` set to `fill` or `justify` set to `between`, as under Layout

### VS Code extension
- Autocomplete and validation cover `column`, `row`, `width="fill"` and `justify="between"`, and the document templates use them

## v0.25.0 — 2026-10-11
### Documents
- Bookmarks: `<bookmark>` adds an entry to the PDF's outline, the viewer's bookmark panel, nested by `level` (1 to 6). It marks the element that follows it, and directly inside a section it marks the section's first page
- Links within the document: `href="#id"` on `<link>` and `<span>` jumps to the bookmark with that id
- A table of contents: `<section type="toc">` is laid out after the other sections, with the data `$toc`, one entry per bookmark with its label, level and page number. Entries are indented by level with `toc-indent-step` (12pt by default), and `href="#"` inside an entry links to its bookmark

### Layout
- `widows` and `orphans` on `<text>`: the fewest lines, 1 to 10, that a paragraph split across pages may leave at the top of the next page or at the bottom of the first
- Both default to 2, so some documents break pages differently than in v0.24.0: a split paragraph keeps at least two lines on each page where it can. `widows="1"` and `orphans="1"` turn this off

### SDKs
- The `L` builder has a `bookmark` element, the section attributes `type` and `tocIndentStep` with a `SectionType` constant, and `widows` and `orphans` on text

### VS Code extension
- Autocomplete and validation cover bookmarks, the table of contents section, and `widows` and `orphans`

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
