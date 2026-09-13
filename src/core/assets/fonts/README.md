# Bundled fonts

## `radley-attribution.ttf`

Radley Regular, cut down to the four glyphs of "Lpdf". Unlicensed renders draw an
attribution line in the page's top-right corner — "Made with", the lpdf mark, "Lpdf" —
and the name is set in this face so it matches the lpdf.io wordmark.

**Why a cut-down copy.** The full face is 93 KB and would ship inside every WASM and WASI
build, licensed users included. The four glyphs come to about 2 KB. Radley has no bold,
so the engine thickens the name with a fill-and-stroke outline instead of bundling a
second face.

**Licence.** SIL Open Font License 1.1 with no Reserved Font Name, so a subset may keep
the name; see `OFL-Radley.txt`. The copyright and licence records stay in the file's
`name` table.

**Regenerating** with fontTools, from the full face the lpdf.io site serves:

```
python -m fontTools.subset Radley-Regular.ttf --text="Lpdf" \
    --output-file=radley-attribution.ttf --layout-features="" --no-hinting \
    --name-IDs="*" --name-languages="*" --drop-tables+=GPOS,GSUB,GDEF,gasp,DSIG
```

Regenerate with the new text if the attribution name ever changes: a test in `pdf.rs`
fails when this face is missing a glyph of the name.
