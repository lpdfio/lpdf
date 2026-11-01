# Colour profiles the fixtures use

`showcase-pdfx.xml` and the brochure example declare this profile in `<assets>` and name it as the press profile of
their PDF/X output, as a document names the profile its printer gives out. The engine bundles no press profile.

## `FOGRA39L_coated.icc`

A printer profile for offset printing on coated paper (FOGRA39, ISO 12647-2:2004/Amd 1): ICC version 2.2, class
`prtr`, CMYK to Lab, 121,368 bytes, with every intent's `A2B` and `B2A` tables and a gamut tag. Its description is
"FOGRA39L Coated".

**Source.** `FOGRA39L_coated.icc` of the CTAN package [colorprofiles](https://ctan.org/pkg/colorprofiles), version
20181105 (`support/colorprofiles.zip` on a CTAN mirror). SHA-256
`b35713ef7eff09349d4c3249e5f377736d06d8a2671c54712971a3546bf17c57`. Peter Selinger generated it in 2018 with
`cd-create-profile` of the colord package, from the colord source `FOGRA39L_coated.iccprofile.xml`, which builds it
from Fogra's published FOGRA39L characterization data.

**Licence.** Free to copy and ship in a public repository:

- The source XML marks the profile `CC0`, and the profile carries the statement "This profile is free of known
  copyright restrictions" in its copyright tag.
- Peter Selinger, who generated the ICC file, waived all his rights to it worldwide, as the package's README quotes
  him ([tex-live, 2018-09](https://tug.org/pipermail/tex-live/2018-September/042340.html)).
- The package's README lists it under the zlib/libpng licence, with the copyright of Kai-Uwe Behrmann, Marti Maria,
  Photogamut, Graeme Gill and ColorSolutions: use for any purpose, and redistribution, are allowed, as long as the
  origin is not misrepresented and an altered version is marked as one. The file here is unaltered.
