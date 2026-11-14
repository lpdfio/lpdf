#!/usr/bin/env node
// Writes the images of the JPEG and PNG tests (`src/core/src/image_render_tests.rs`) into this folder, each made from
// the pattern of `../formats/make.mjs`: 40 by 30 pixels of 16 colors in blocks of 5 by 5. Run it again only to change
// the images:
//
//     node test/assets/images/jpeg-png/make.mjs
//
// Needs on the PATH: cjpeg (libjpeg-turbo) and sips (macOS). The CMYK JPEG is sips' own conversion of
// `../formats/tiff-cmyk.tif`, so it carries what a CMYK JPEG from a photo tool does: four components, an Adobe marker
// and a profile.

import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import zlib from 'node:zlib';

const here = dirname(fileURLToPath(import.meta.url));
const at = (name) => join(here, name);
const run = (tool, ...args) => execFileSync(tool, args, { stdio: ['ignore', 'ignore', 'inherit'] });

const WIDTH = 40;
const HEIGHT = 30;
const PALETTE = [
    [0, 0, 0], [255, 255, 255], [255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0], [0, 255, 255], [255, 0, 255],
    [128, 0, 0], [0, 128, 0], [0, 0, 128], [128, 128, 0], [0, 128, 128], [128, 0, 128], [192, 192, 192], [128, 128, 128],
];
const index = (x, y) => (Math.floor(x / 5) + Math.floor(y / 5) * 3) % 16;

// ── JPEG ─────────────────────────────────────────────────────────────────────

// Gray: each pattern index as 17 times it, written as a PGM for cjpeg to compress to one component.
const scratch = mkdtempSync(join(tmpdir(), 'lpdf-jpeg-png-'));
const pgm = join(scratch, 'gray.pgm');
const gray = [];
for (let y = 0; y < HEIGHT; y++) {
    for (let x = 0; x < WIDTH; x++) gray.push(index(x, y) * 17);
}
writeFileSync(pgm, Buffer.concat([Buffer.from(`P5\n${WIDTH} ${HEIGHT}\n255\n`, 'latin1'), Buffer.from(gray)]));
run('cjpeg', '-grayscale', '-quality', '95', '-outfile', at('gray.jpg'), pgm);
rmSync(scratch, { recursive: true });

run('sips', '-s', 'format', 'jpeg', join(here, '..', 'formats', 'tiff-cmyk.tif'), '--out', at('cmyk.jpg'));

// ── PNG ──────────────────────────────────────────────────────────────────────

function chunk(type, body) {
    const typed = Buffer.concat([Buffer.from(type, 'latin1'), body]);
    const length = Buffer.alloc(4);
    length.writeUInt32BE(body.length);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(zlib.crc32(typed));
    return Buffer.concat([length, typed, crc]);
}

// A PNG of the pattern: `colorType` 2 is RGB and 3 a palette of the 16 colors, `interlace` 1 is Adam7, `trns` the
// palette's alpha entries. `pixel(x, y)` is the bytes of one pixel.
function png({ colorType, interlace = 0, trns = null, pixel }) {
    const header = Buffer.alloc(13);
    header.writeUInt32BE(WIDTH, 0);
    header.writeUInt32BE(HEIGHT, 4);
    header[8] = 8;
    header[9] = colorType;
    header[12] = interlace;
    // Adam7: seven passes, each a smaller image of every dx-th pixel of every dy-th row from (x0, y0).
    const passes = interlace === 1
        ? [[0, 0, 8, 8], [4, 0, 8, 8], [0, 4, 4, 8], [2, 0, 4, 4], [0, 2, 2, 4], [1, 0, 2, 2], [0, 1, 1, 2]]
        : [[0, 0, 1, 1]];
    const rows = [];
    for (const [x0, y0, dx, dy] of passes) {
        for (let y = y0; y < HEIGHT; y += dy) {
            if (x0 >= WIDTH) break;
            rows.push(Buffer.from([0]));
            for (let x = x0; x < WIDTH; x += dx) rows.push(Buffer.from(pixel(x, y)));
        }
    }
    return Buffer.concat([
        Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
        chunk('IHDR', header),
        ...(colorType === 3 ? [chunk('PLTE', Buffer.from(PALETTE.flat()))] : []),
        ...(trns ? [chunk('tRNS', Buffer.from(trns))] : []),
        chunk('IDAT', zlib.deflateSync(Buffer.concat(rows))),
        chunk('IEND', Buffer.alloc(0)),
    ]);
}

const rgb = (x, y) => PALETTE[index(x, y)];
const entry = (x, y) => [index(x, y)];

writeFileSync(at('interlaced.png'), png({ colorType: 2, interlace: 1, pixel: rgb }));
writeFileSync(at('palette.png'), png({ colorType: 3, pixel: entry }));
// Entry 0, black, is transparent, and entry 1, white, half so: the other 14 are opaque.
writeFileSync(at('palette-trns.png'), png({ colorType: 3, trns: [0, 128], pixel: entry }));
