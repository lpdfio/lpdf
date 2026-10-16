# Changelog

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
