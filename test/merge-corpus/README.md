# Merge corpus

PDFs that `merge` is checked on, from the tools people print and save with: the core's tests merge each file alone
(`merge_tests.rs`, `lopdf_reads_every_file_of_the_corpus_and_the_merge_takes_each`), and `make check-merge` merges every
file, alone and all in one PDF, then opens the output with qpdf and renders each page of it, and of its source, with
pdf.js (`scripts/check-merge.mjs`). Files built for the structures these lack, such as pages turned or cut by a crop box,
annotations of each flag combination and layers, are written by the tests themselves (`synthetic` in `merge_tests.rs`).

The producer and version are what each file's own `/Producer` and header say.

| File | Producer | PDF | Pages | Source | License |
|---|---|---|---|---|---|
| `irs-fw9.pdf` | Designer 6.5, a fillable form | 1.7 | 6 | [irs.gov/pub/irs-pdf/fw9.pdf](https://www.irs.gov/pub/irs-pdf/fw9.pdf), Form W-9 | Work of the US Government, public domain |
| `irs-f8822.pdf` | Adobe LiveCycle Designer ES 9.0, a fillable form | 1.7 | 2 | [irs.gov/pub/irs-pdf/f8822.pdf](https://www.irs.gov/pub/irs-pdf/f8822.pdf), Form 8822 | Work of the US Government, public domain |
| `irs-f8949.pdf` | Designer 6.5, a fillable form | 1.7 | 2 | [irs.gov/pub/irs-pdf/f8949.pdf](https://www.irs.gov/pub/irs-pdf/f8949.pdf), Form 8949 | Work of the US Government, public domain |
| `uscis-i-9.pdf` | Acrobat Distiller 23.0 (Windows), a fillable form | 1.7 | 4 | [uscis.gov/sites/default/files/document/forms/i-9.pdf](https://www.uscis.gov/sites/default/files/document/forms/i-9.pdf), Form I-9 | Work of the US Government, public domain |
| `irs-f1040es-pages-1-3.pdf` | Antenna House PDF Output Library 6.6 | 1.7 | 3 | Pages 1 to 3 of [irs.gov/pub/irs-pdf/f1040es.pdf](https://www.irs.gov/pub/irs-pdf/f1040es.pdf), Form 1040-ES, cut with `qpdf --pages` | Work of the US Government, public domain |
| `nist-cswp-csf-pages-1-4.pdf` | Microsoft Word 2013 | 1.6 | 4 | Pages 1 to 4 of [nvlpubs.nist.gov/nistpubs/CSWP/NIST.CSWP.04162018.pdf](https://nvlpubs.nist.gov/nistpubs/CSWP/NIST.CSWP.04162018.pdf), NIST Cybersecurity Framework 1.1, cut with `qpdf --pages` | Work of the US Government (NIST), public domain in the US |
| `govinfo-cfr-21-1-1.pdf` | iText Core 7.2.3, and digitally signed by the Government Publishing Office | 1.7 | 1 | [govinfo.gov](https://www.govinfo.gov/content/pkg/CFR-2023-title21-vol1/pdf/CFR-2023-title21-vol1-sec1-1.pdf), 21 CFR 1.1 | Work of the US Government, public domain. It has a defect of the kind real files have: the trailer's object count is not one more than the highest object number |
| `chrome-skia-agreement.pdf` | Skia/PDF m157 (Chrome's print to PDF) | 1.4 | 2 | Made for this corpus: `chrome --headless --print-to-pdf` of a two-page HTML page of a table, a rounded box with opacity, and an SVG path | Made by the Lpdf authors, under the license of this repository |
| `macos-quartz-text.pdf` | macOS Quartz PDFContext (`cupsfilter`) | 1.3 | 2 | Made for this corpus: `cupsfilter` of a plain text file, macOS's print system | Made by the Lpdf authors, under the license of this repository |
| `lpdf-showcase-forms.pdf` | Lpdf: a filled and empty text field, check boxes, a dropdown, radio buttons and a submit button, each with its stored appearance | 1.7 | 1 | `lpdf convert` of `test/fixtures/showcase-forms.xml` | Made by the Lpdf authors, under the license of this repository |
| `lpdf-showcase-ocg.pdf` | Lpdf: content in a print layer and a screen layer | 1.7 | 1 | `lpdf convert` of `test/fixtures/showcase-ocg.xml` | Made by the Lpdf authors, under the license of this repository |

## What the corpus says about what Lpdf reads

`lopdf` reads every file here, each merged result passes `qpdf --check` without a warning, and each page renders in
pdf.js as its source page does, with its layers and its annotations as they print. So the docs name these producers and
versions and no others: Microsoft Word 2013, Adobe Acrobat Distiller 23, Adobe LiveCycle Designer ES 9.0, Designer 6.5, Antenna House, iText 7,
Skia/PDF (Chrome), macOS Quartz and Lpdf itself; PDF 1.3, 1.4, 1.6 and 1.7. Producers the corpus lacks, scanners and
LibreOffice among them, and PDF 2.0, are not named.
