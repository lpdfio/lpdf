# The press profile of this example

The brochure is written as PDF/X-4 for offset printing on coated paper, and names this profile as its output intent
with `<profile src>` in `<assets>` and `<output pdfx="4" profile="press">`, as a document names the profile its
printer gives out. Ask the printer for theirs, and use it in its place.

`FOGRA39L_coated.icc` is a printer profile for FOGRA39 (ISO 12647-2:2004/Amd 1): ICC version 2.2, CMYK, 121,368
bytes, "FOGRA39L Coated". It comes from the CTAN package [colorprofiles](https://ctan.org/pkg/colorprofiles), version
20181105, which Peter Selinger generated with colord's `cd-create-profile` from Fogra's published FOGRA39L
characterization data. SHA-256 `b35713ef7eff09349d4c3249e5f377736d06d8a2671c54712971a3546bf17c57`.

It is free to copy and ship: its source marks it CC0, it says "This profile is free of known copyright restrictions"
in its copyright tag, Peter Selinger waived his rights to the file, and the package lists it under the zlib/libpng
licence, which allows any use and redistribution of an unaltered copy.
