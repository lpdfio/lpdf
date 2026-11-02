# veraPDF corpus files

Four PDF/A-1b files from the [veraPDF test corpus](https://github.com/veraPDF/veraPDF-corpus) (branch `staging`,
folder `PDF_A-1b`), which the stamp tests stamp and `make check-pdfa` hands to veraPDF: a stamped PDF/A-1b file must
still be one. Each is a "pass" file of the rule its name gives, renamed from `veraPDF test suite <name>.pdf`:

| File | Rule it passes |
|---|---|
| `6-1-3-t01-pass-a.pdf` | 6.1.3, the file trailer |
| `6-1-4-t03-pass-a.pdf` | 6.1.4, the cross-reference table |
| `6-2-3-3-t01-pass-a.pdf` | 6.2.3.3, uncalibrated colour, with a monochrome output intent |
| `6-4-t01-pass-a.pdf` | 6.4, transparency |

The files are from the veraPDF corpus by [veraPDF](https://verapdf.org), licensed under
[Creative Commons Attribution 4.0 International](https://creativecommons.org/licenses/by/4.0/) (CC BY 4.0). They are
as the corpus has them; only their names changed.
