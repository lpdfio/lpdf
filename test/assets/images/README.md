# Images of the tests

The pictures the fixtures in `test/fixtures` name by relative path, and the tests and benches read.

- `chapter-05-caterpillar.jpg`: a colour plate by Arthur Rackham from the 1907 edition of Alice's Adventures in
  Wonderland, the same file as in `examples/6-book/assets/images`. The book was published in 1907 and the artist
  died in 1939, so the plate is in the public domain in the United States.
- `formats/`: the WebP, GIF, TIFF and BMP files of the image-format tests, each made from one pattern of 40 by 30
  pixels by `formats/make.mjs`, which says how.
- `svg/`: small SVGs written by hand for the SVG tests, each made to hold one part of what the engine draws: `shapes` (every
  basic shape, strokes, dashes, fill rules), `structure` (`use`, `<style>`, a clip, opacity on a shape and on a group),
  `current-color` and `current-color-root` (`currentColor`, with and without a `color` on the root), `no-size` (no
  `width`, `height` or `viewBox`), `view-box` (a `viewBox` that doesn't start at the origin) and `marker` (a marker at each
  vertex of a polyline).
