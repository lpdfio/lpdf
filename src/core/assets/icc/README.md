# Bundled colour profile

## `sRGB-v2-magic.icc`

The one ICC profile inside the engine: sRGB, 736 bytes. A PDF/A file names it as its output intent, which gives
the RGB colours and images of the document a defined meaning. It is the only profile the engine bundles; a press
profile for PDF/X is the document's own asset.

**Why this one.** It is an ICC version 2 display profile (`mntr`, `RGB `), which every PDF/A part accepts and the
widest range of readers understands, with a 182-point tone curve that matches the sRGB curve closely while staying
under 1 KB. The full profiles that operating systems ship are about 3 KB.

**Source.** `profiles/sRGB-v2-magic.icc` of [saucecontrol/Compact-ICC-Profiles](https://github.com/saucecontrol/Compact-ICC-Profiles),
commit `abb1b6f48570b4eb0fabd8566e517dca07e617d4`. SHA-256
`af4efe28f6d311799865f325ba39184a2e978b113ca74124d60af7de22b105f4`.

**Licence.** Released to the public domain under Creative Commons CC0 1.0, as the source's README and licence say:
free to copy, change and ship, with no attribution required.
