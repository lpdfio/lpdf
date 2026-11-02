# Files the fixtures carry

`showcase-attachments.xml` embeds these files in its PDF with `<attachments>`. Both were written for the tests and
are free to use.

| File | What |
|---|---|
| `invoice.xml` | The data of the invoice the fixture shows: `relationship="data"`, `text/xml` |
| `terms.txt` | Its terms of sale: `relationship="supplement"`, `text/plain` |

## facturx

`showcase-facturx.xml` and the cases of `facturx_render_tests.rs` carry these files in Factur-X invoices. All were
written for the tests and are free to use; their data is made up. The package of the Factur-X spec has its own
samples, which are not free to use and are not here.

| File | What |
|---|---|
| `minimum.xml`, `basic-wl.xml`, `basic.xml`, `en16931.xml`, `extended.xml` | The invoice in CII, one for each profile of Factur-X, which BT-24 names |
| `xrechnung-3.0.xml`, `xrechnung-2.1.xml` | An invoice of the XRECHNUNG profile, to a German public body, in XRechnung 3.0 and 2.1 |
| `en16931-spaces.xml` | `en16931.xml` with spaces and line breaks around its BT-24 |
| `en16931-prefix.xml` | `en16931.xml` with the namespace of BT-24 under the prefix `a`, not `ram` |
| `factur-xubl.xml`, `factur-xedifact.edi` | The invoice in UBL and in EDIFACT, in part, under the names the spec gives them |
| `lines.xlsx`, `delivery-note.pdf` | A spreadsheet and a PDF, formats the spec lists for the files beside the invoice |
| `notes.txt` | A file the cases carry with no `mime` |
