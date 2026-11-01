# Changelog

## v0.31.0 — 2026-11-01
### Attributes
- **Breaking:** every attribute is checked against the schema before the document is read, in XML and in a document built in code. A value that isn't of its type is an error that names the element, the attribute and the type: `<grid> at line 3, col 5: cols must be a whole number from 1 to 12, not "x"`. A boolean is `true` or `false`: `bold="1"` and `bold="yes"`, which read as false, are errors, and the VS Code extension flags them as you type. An attribute the element doesn't have, such as `agle` on a gradient, was ignored and is now an error that lists the attributes the element has
- **Breaking:** in the SDKs' builders an attribute takes its own type: `bold: true`, `cols: 3`, `opacity: 0.5`, and `lineHeight` a factor such as `1.5` or a token name such as `'m'`. `bold: 'true'` doesn't compile in TypeScript or C#, and a number written as a string, such as `cols: '3'`, is an error when the document renders. In JavaScript, a `NaN` or an infinity is refused when the document is built

### Encryption
- **Breaking:** a document says that it is encrypted, on a new `<output>` element after `<meta>`: `<output encrypt="aes-256" allow="print fill-forms"/>`, where it was set on the engine. `encrypt` is `aes-256` or `rc4-128`. `allow` lists what a reader may do without the owner password, separated by spaces: `print`, `modify`, `copy`, `annotate`, `fill-forms`, `accessibility`, `assemble`, or `all`. A permission comes only from naming it, so without `allow`, or with `allow=""`, a reader may do nothing without the owner password. `print` allows printing at full quality: printing at low quality only is no longer offered
- **Breaking:** the passwords come with each render, since they change from one render to the next and can't sit in a template: `userPassword` and `ownerPassword` in the render options (`UserPassword` and `OwnerPassword` in .NET, `user_password` and `owner_password` in Python). `setEncryption`, `clearEncryption` and their option types are gone from every SDK. A document that asks for encryption and gets no owner password is an error, and so are passwords for a document that doesn't ask for it, so a file never goes out unencrypted while the caller believes it is locked. `skipEncryption` renders a document that asks for encryption without it, for a preview
- RC4-128 keeps strings intact. A title, or any text the PDF stores as a string, could lose its value or break its object when its encrypted bytes held a parenthesis or a backslash, and `C:\reports` came back with its backslashes doubled

### PDF/A
- `pdfa="2b"` or `pdfa="2u"` on `<output>` writes PDF/A-2b or PDF/A-2u (ISO 19005), the PDF that archives, courts, public bodies and banks ask for, made to look the same decades from now. The file carries XMP metadata, the sRGB output intent, its fonts and a file ID, and the same document gives the same bytes. veraPDF, the reference validator, passes both levels
- A PDF/A render needs a creation date, `createdOn` in the render options, as an ISO 8601 date, or date and time with an optional zone
- What PDF/A forbids is an error that says what to change, since the engine changes nothing in the document by itself: text in a built-in font (embed a font instead), a font whose license forbids embedding, a character its font has no glyph for, a CMYK color, a form field, a barcode's printed digits, and encryption

### Attachments
- `<attachments>`, after `<output>`, embeds files in the PDF, which a reader lists in its attachments panel: `<file name="invoice.xml" src="invoice.xml" mime="text/xml" relationship="data"/>`. An e-invoice carries its XML for software to read, and a report can carry its source or its terms. `relationship` is `source`, `data`, `alternative`, `supplement` or `unspecified`, and `description` is the description a reader shows for it
- The SDK reads a file's `src`, or takes the bytes from `loadFile(name, bytes)` on the engine (`LoadFile` in .NET, `load_file` in Python), under the file's `ref` or its name. An encrypted PDF encrypts its files with the rest. Attachments in a PDF/A document are an error, since PDF/A-2 may carry only files that are PDF/A themselves

### Colors
- **Breaking:** a malformed color is an error that names it. A hex color with a character that isn't a hex digit, such as `#ggg`, was drawn black
- A color can be written as `cmyk(C, M, Y, K)` with values from 0 to 100, `rgb(R, G, B)` from 0 to 255, or `hsl(H, S%, L%)`, wherever a color goes: attributes, tokens, `data-color`, borders and shadows. The values are separated by commas or by spaces. `cmyk()` is written to the PDF as CMYK, so a printer separates its plates without converting from RGB, which shifts colors
- A gradient's colors are all CMYK or all RGB. A value out of its range is an error, and so is an alpha, which points to `opacity`

### Fonts
- An OpenType font with CFF outlines (most `.otf` files) is embedded in the form PDF defines for it, where it was written as a TrueType font, which a reader may draw wrongly. A variable font with CFF2 outlines is embedded as a subset at its default instance, where it was embedded whole and macOS drew each of its glyphs as a dot

### Footnotes
- Content taller than a page that can't split, such as a frame or a grid row, is cut at the bottom of the page. A footnote whose number falls below the cut now has no note, on that page or any other, and its note takes no room, so the page shows more of the content. With `footnote-counter="page"` the numbers shown run 1, 2, … in reading order; with `section` or `document`, the numbers cut off are skipped

### Metadata
- The creation date is written as a date a PDF reader understands, with the precision and time zone it is given in. A date with a fraction, such as `2026-10-31T09:30:00.000Z` from JavaScript's `toISOString()`, or with an offset such as `+01:00`, gave a date readers showed wrongly or not at all. A value that isn't a date is now an error that names it and gives the form, where `31.10.2026` gave a wrong year

### Command line
- The command line takes no passwords, since a password on it shows in `ps` to anyone on the machine. A document that asks for encryption is an error unless `convert` or `benchmark` is given `--skip-encryption`, which renders it unencrypted
- `convert` reads the `src` of attached files relative to the document

### SDKs
- **Breaking:** in .NET, `Render` returns `byte[]` and `L.ToXml` returns `string`, on the caller's thread: drop the `await`. To render on another thread, write `await Task.Run(() => engine.Render(doc))`
- **Breaking:** the Python SDK needs Python 3.11 or later, which its enums need; it said 3.10
- **Breaking:** the PHP SDK needs the `dom` extension, which most PHP builds include and some Linux distributions package apart (`php-xml`)
- The builders have `output` (`encrypt`, `allow` and `pdfa`) and `attachments` on the document's attributes, with the constants `Cipher` and `Permission`, and `FileAttr` for an attached file
- The SDKs read the `src` of a font, an image, Markdown styles and an attached file as the engine reads the XML: entities are decoded, and a comment or a CDATA section is left out, so an element shown as text in a code block is not loaded

### VS Code extension
- **Breaking:** **Lpdf: Export Encrypted PDF...** and the `lpdf.encryptionAlgorithm` setting are gone. **Export PDF** encrypts a document whose `<output>` asks for it, asking for the owner password and an optional open password, each typed twice. The preview renders it unencrypted, and says so once
- **Preview PDF** and **Export PDF** write PDF/A for a document that asks for it, dated with the time of the render, and embed the files of `<attachments>` from their `src`, beside the document or at the root of its workspace folder. The preview follows an edit of an attached file
- **Lpdf: Compare PDF with branch...** shows a PDF beside its version on a branch or a tag, next to **Compare PDF with HEAD** in the Explorer's menu and in the PDF tab's menu
- **Generate Code** writes each attribute as a literal of its type (`cols: 3`, `bold: true`, `True` in Python), and its C# compiles: it imports `NoAttr` with `using static Lpdf.L;` and calls `Render` without `await`
- Hover, autocomplete and validation follow the schema, so they cover `<output>`, `<attachments>`, the new color forms and the boolean values
- Lpdf installs from Open VSX, the extension registry of Cursor, VSCodium and other editors built on VS Code

## v0.30.0 — 2026-10-30
### Markdown
- `<markdown>` takes the place of the `<layout>` in a section and sets Markdown as a styled, paginated PDF: CommonMark with tables, strikethrough and footnotes. It is for a body written in Markdown, such as release notes, an article or documentation, or one that comes from a CMS, a database or a user. The page, a cover, a header and a footer stay in the XML: a `<layer>` draws a header or a footer in the margin the Markdown leaves. The text is inline in `<markdown-content>`, in a CDATA section, or comes from the data with `data-value`
- Each part becomes the element it stands for: a heading is a text kept with the block after it, and a bookmark, so the viewer's outline and a table of contents fill themselves and `[the methods](#3-1-methods)` links to it; a paragraph is a text; emphasis, strong and struck-through words are spans; inline code is a `code-span` and a code block a `<code>`; `---` is a divider; a quote has a bar beside it; lists nest; `- [ ]` and `- [x]` are checkboxes; a table has a header row that repeats on each page and columns as wide as their cells need; `[^1]` is a footnote. Two spaces or a `\` at the end of a line, or a `<br>`, start a new line in the same paragraph, list item, quote or table cell. A `$` is text: there is no math
- `<markdown-styles>` holds rules named after the Markdown they style, `h1` to `h6`, `p`, `code`, `quote`, `li`, `hr`, `a`, `strong`, `em`, `del`, `table`, `th` and `td`, which take the attributes of the element they become: `<h1 color="primary"/>`. `src` reads the rules from a file, and the rules written inline are laid over it one attribute at a time. With no styles, the default look uses token names for its colors, so a token file alone rebrands it. In an embedded font, the `em` and `strong` rules name the italic and bold faces
- An image names an asset by its `name` or by its `src`, so `![](img/photo.png)` keeps the relative path a `.md` file has everywhere else. An image at a URL is left out, since Lpdf does not download
- Raw HTML is left out, with the words in it kept: a block such as `<p>` or `<div>` puts the words around it on lines of their own, and a comment, a `<script>` or a `<style>` leaves nothing. Two tags are read: `<br>` breaks a line, and `<img src width height>` is an image at that size, in pixels with or without `px`, a pixel being 0.75pt. A README with a centered logo and a row of badges renders, without the badges
- A mistake is an error that names the line of the Markdown and says what to write instead: an image that is not an asset, a footnote with no definition or in a table, a rule that does not exist
- A `.md` file is set in a template, a document with a `<markdown>` that reads the file from the data `markdown`. The `key: value` lines of its YAML front matter are data beside it, for a template to show, and its relative images are read relative to the file. With no template of its own, it is set on an A4 page with a 54pt margin and the page number at the foot. The command line and the VS Code extension convert one

### Lists
- `<list>` holds `<li>` items, each marked with a bullet or a number the engine writes. `type="bullet"` draws a shape, the same in every font: a disc, a circle in a list inside it and a square below that, or the one `bullet` names (`disc`, `circle`, `square`, `dash`). `type="number"` writes `decimal`, `lower-alpha`, `upper-alpha`, `lower-roman` or `upper-roman` numbers, from `start`, with a `prefix` and a `suffix` (`.` by default). `indent` is the width of the marker's column, `gap` the space between items
- An item holds what a layout holds, and a list inside an item nests under it. A long item wraps under its own words, not under the marker, and splits across pages like a text, the marker staying with the first part. `data-source` on an `<li>` makes an item for each entry of the data, numbered in order
- `<li checked="true">` and `checked="false"` draw a checkbox in place of the marker: an outline in the `line` color, ticked in the color of the item's words when it is checked

### Code
- `<code>` is a box for code, or any text whose spaces and lines matter. It keeps every space and line as written, wraps a line longer than the box at a space and breaks a word with none at the last character that fits, and splits across pages. A tab fills to the next multiple of `tab-size` columns, 4 by default. It is set in Courier on a light tint with padding, which `font`, `font-size`, `color`, `background`, `padding`, `border` and `radius` change, and a `<span color>` inside it colors a key word. The tags can stand on lines of their own, as `<pre>` does in HTML
- `<code-span>` inside a `<text>` is the same look in a line: `Run <code-span>cargo test</code-span> first`

### Tables
- `auto` in `cols` makes a column as wide as its cells need on one line, so a column of numbers is as narrow as its numbers: `cols="auto 90pt auto"`. The `auto` columns share what the `pt` and `%` columns leave, and when they do not fit the longest names break first. `auto` with `fr` is an error that names the table. A table split across pages keeps the widths of its first page
- `head-border` draws a line under the header row and `row-border` one under each row, as a thickness and a color like `border`: `head-border="xs line" row-border="xs line-muted"`. A line stops where a cell spans into the next row, and a header repeated on the next page has its line too. `none` turns `stripe`, `head-border` or `row-border` off
- Two color tokens for lines: `line`, a mid grey (`#737373`), and `line-muted`, a light one (`#d4d4d4`). A document can give them other values in its tokens

### Text
- A word, or a styled run, wider than a whole line breaks at the last character that fits, so nothing runs past its box or out of a table cell
- A built-in font draws `≥`, `≤`, `≠` and `≈`, which were drawn as `?`
- Text in an embedded font is measured with each character's own width, where layout used one average width for anything outside ASCII. Lines with accented letters, curly quotes, dashes or a non-Latin script wrap where the text ends, and a span after such a character sits where it should
- `align="justify"` justifies text in an embedded font, which came out ragged before. A justified line places its words, so a bold, linked or colored span moves with its neighbours instead of overlapping them
- Text in a built-in font is measured with the fonts' own widths. An apostrophe was measured as wide as a curly quote, and a few letters and most characters above ASCII were approximate, so a line can now hold a word more than before. A character a built-in font draws as `?` is measured as wide as the `?`
- A space at the start of a `<span>` that follows another span or a `<code-span>` is kept: `<span>a</span><span> b</span>` reads `a b`

### Layout
- **Breaking:** a `<divider>` with no `color` or `thickness` is a thin line in the `line` color, 0.5pt in a mid grey, where it was a 1pt black line. Set `color="#000000" thickness="s"` to keep the old line
- A row splits across pages through a line: each child stops at its last line above the page's end and goes on at the top of the next page, at the width it had. A long bullet made of a marker and a text breaks inside its text instead of moving whole and leaving a gap
- Content taller than a page, such as an image, a frame, a grid row or a box that does not split, is cut at the bottom of the page, where it was drawn over the footer and past the page's edge. A column or a grid goes on after it on the next page
- `paginate="keep-next"` lets an element that splits start on the page: a table after its header and a row, a text after a few lines, with the element before it staying on that page. Before, both moved to the next page whenever the second did not fit whole

### Data
- An attribute takes its value from the data with a `data-` form of its name, the same dot-paths, loop scopes, `/` and `../` as `data-value`: `data-href` on a `<link>`, `data-name` on an `<img>` or `<img-at>` (one of the images declared in `assets`), `data-color` on a text, a box, a divider or a barcode, `data-background` on a box or a barcode, and `data-fill` on a `<shape>`. The attribute without `data-` stays when there is no data or the path has no value. A value that is not a string, a color that does not read or an image that is not declared is an error that names the attribute and the path

### Encryption
- **Security:** encryption is AES-256 by default, the cipher PDF 2.0 asks for, with the same user and owner passwords and the same eight permission flags. The SDK sends the engine random bytes from the platform's secure generator, so rendering the same document twice gives two different files. Compare, cache or sign the bytes before the encryption, or with RC4-128
- **Deprecated:** RC4-128, which PDF 2.0 deprecates. It stays available for a reader that cannot open AES-256: `algorithm: 'rc4-128'` in Node.js and PHP, `algorithm='rc4-128'` in Python, `Algorithm = EncryptAlgorithm.Rc4128` in .NET

### Performance
- Performance improvements in layout and parsing: documents render faster, long ones most of all

### Command line
- `lpdf convert --data data.json` renders a document bound to data, as the SDKs do
- `lpdf convert --input notes.md --output notes.pdf` converts a Markdown file, with `--template` for a template of your own and `--data` for fields beside those of its front matter

### SDKs
- The builders have the new elements: `L.list()` and `L.li()`, `L.code()` and `L.codeSpan()`, and `L.markdown()` with `LMarkdown` for its styles, rules and content, in each language's case (`L.code_span()` and `LMarkdown.del_` in Python, `L.CodeSpan()` in .NET). The `data-` attributes and `checked` on an item come with them
- The encryption options take an `algorithm`, `aes-256` or `rc4-128`

### VS Code extension
- **Breaking:** the data of `invoice.xml` is found by name in `invoice.data.json`, no longer in `invoice.json`. Rename the file, or link it with **Lpdf: Link Data File...**
- **Lpdf: Preview PDF** on a `.md` file shows it as a PDF beside it, and **Export PDF** writes one. The preview follows an edit of the Markdown, its styles, its document and its images, and scrolls with the editor both ways, matching them at each heading
- **Lpdf: Link Markdown Styles...** offers three looks, Sans, Serif and Typewriter, and writes the one picked beside the file as `notes.styles.xml`, which the preview uses and the author edits. **Lpdf: Create Lpdf Document** writes a document for the file, A4 with a page number, where the page, a header and a footer are yours, and **Lpdf: Link Markdown File...** makes a `.md` file the body of a document with a `<markdown>`
- **Lpdf: Export Encrypted PDF...** asks for an owner password, an optional open password and the permissions, then writes the PDF encrypted. The passwords are asked for each time and never stored. The setting `lpdf.encryptionAlgorithm` picks `aes-256` (the default) or `rc4-128`
- Hover, autocomplete and validation follow the schema, so they cover the new elements and attributes

## v0.29.0 — 2026-10-23
### Tables
- `colspan="n"` on a `<td>` makes the cell as wide as its `n` columns and the gaps between them, so a subtotal label or a full-width heading can sit in a table without nesting one. The cells after it in the row start at the column after its last. A row whose cells span more columns than the table has is an error; a row with fewer leaves its last columns empty. It changes widths only: row heights and page breaks work as before
- `rowspan="n"` on a `<td>` makes the cell as tall as its `n` rows and the gaps between them, for a row-group label or a note that runs down the side. The rows under it leave its columns out, so their cells start at the first free columns. If the cell's content is taller than the rows are together, the last of them grows by the difference. The rows a cell spans stay on one page: the group moves to the next page whole when it does not fit, and one taller than a page is clipped at the page's end. A row with more cells than free columns, and a span that runs past the last row, are errors. A `<thead>` is one row and takes no `rowspan`
- `keep="with-next"` on a `<tr>` keeps the row on the same page as the row after it, so a line item is not left behind its total. A run of rows that each say `with-next`, and the row that ends it, move to the next page together when a page break would fall inside them. A run taller than a page breaks where its rows fit, as if it had no `keep`. Rows are never split, and a header is never left alone at the bottom of a page, with or without it

### Text
- Footnotes. A `<fn>` inside a `<text>` in the layout shows a small raised number at its place and puts its words at the bottom of the page where that number lands, under a short rule, in the order of the numbers. The engine numbers the notes, and the number stays with the word before it at the end of a line. A page with notes has less room for the text, so the line with the number moves to the next page, with its note, when the note does not fit. A note taller than a whole page is cut at the page. A note holds words and `<span>`s
- `footnote-counter` on the document or a `<section>` says where the numbers start over: `page` for every page, `section` for every section (the default), or `document` so that they run on through the document. A section's own value overrides the document's
- A `<fn>` is only for a `<text>` in the layout. In a `<text-at>`, a table or a `<region>` it is an error that says so

### Layout
- `<shape type="rect">`, `"circle"` or `"ellipse"` in the layout, for a status dot, a color swatch or a progress bar. It takes room in the flow as an `<img>` does, has no children, and is drawn centered in its box, which the parent's `align` places. A `rect` and an `ellipse` need a `width` and a `height`, a `circle` an `r`. `radius` rounds the corners of a `rect`. `fill`, `stroke` and `opacity` work as on a layer's shapes. A shape with no fill and no stroke draws nothing but keeps its box. A stroke is centered on the edge of the box, so half of it lies outside. To put a shape at a place on the page, use a `<rect>`, `<circle>` or `<ellipse>` in a `<layer>`

### Tokens
- A `height` row in the tokens, `xs` to `xxl`, so that `height="m"` works. Boxes, tables, `<img>`, `<shape>` and barcodes take a name from it, as well as a length. It is a row of its own, because the `width` row is a page-wide scale that gives no heights for a card or a row
- Aliases. `<aliases><alias name="caption" value="xs"/></aliases>` in the tokens gives a slot of the scale a name of your own. An attribute that takes a slot takes the name instead, from its own row: `font-size="caption"` is the text size `xs` and `gap="caption"` is the space `xs`. A name is lowercase letters, digits and `-`, starting with a letter, and not `xs` to `xxl`, `fill` or `full`; its value is a slot, not another alias. A border takes one too: `border="hairline primary"`
- **A name that is neither a slot nor an alias is an error from the engine, not from the schema.** The scale attributes accept any name now, so a typo such as `medum` is no longer flagged by validation against the schema; the engine reports it as an unknown name

### SDKs
- The four SDKs carry the new attributes and elements: `colspan` and `rowspan` on a cell, `keep` on a row, `footnote-counter` on a document and a section (with the values `page`, `section` and `document` as constants), a footnote builder and a shape builder (`L.fn()` and `L.shape()` in JavaScript)
- The tokens take `aliases`, a map from a name to a slot, and a `height` row

### VS Code extension
- Hover, autocomplete and validation follow the schema, so they cover the new elements, attributes and token rows

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
