#!/usr/bin/env node
// Writes the images of the image-format tests (`src/core/src/raster_render_tests.rs`) into this folder: two sources,
// `source.png` and `source-alpha.png`, then each format made from the same pattern, by hand or with a tool.
//
// The pattern is 40 by 30 pixels of 16 colors in blocks of 5 by 5, so a lossless format compares pixel by pixel and
// fits a palette of 4 bits; the tests compute the same pattern. Run it again only to change the images:
//
//     node test/assets/images/formats/make.mjs
//
// Needs on the PATH: cwebp, dwebp and img2webp (libwebp), tiffcp (libtiff) and sips (macOS).

import { execFileSync } from 'node:child_process';
import { rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import zlib from 'node:zlib';

const here = dirname(fileURLToPath(import.meta.url));
const at = (name) => join(here, name);
const run = (tool, ...args) => execFileSync(tool, args, { stdio: ['ignore', 'ignore', 'inherit'] });
const write = (name, bytes) => writeFileSync(at(name), bytes);

const WIDTH = 40;
const HEIGHT = 30;
const PALETTE = [
    [0, 0, 0], [255, 255, 255], [255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0], [0, 255, 255], [255, 0, 255],
    [128, 0, 0], [0, 128, 0], [0, 0, 128], [128, 128, 0], [0, 128, 128], [128, 0, 128], [192, 192, 192], [128, 128, 128],
];
const index = (x, y) => (Math.floor(x / 5) + Math.floor(y / 5) * 3) % 16;
const rgb = (x, y) => PALETTE[index(x, y)];
// Alpha grows from 0 on the left to 255 on the right.
const alpha = (x) => Math.round((x * 255) / (WIDTH - 1));

function pixels(width, height, each) {
    const out = [];
    for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
            out.push(...each(x, y));
        }
    }
    return Buffer.from(out);
}

// ── PNG ──────────────────────────────────────────────────────────────────────

function png(width, height, channels, data) {
    const stride = width * channels;
    const raw = Buffer.alloc((stride + 1) * height);
    for (let y = 0; y < height; y++) {
        data.copy(raw, y * (stride + 1) + 1, y * stride, (y + 1) * stride);
    }
    const chunk = (type, body) => {
        const typed = Buffer.concat([Buffer.from(type, 'latin1'), body]);
        const length = Buffer.alloc(4);
        length.writeUInt32BE(body.length);
        const crc = Buffer.alloc(4);
        crc.writeUInt32BE(zlib.crc32(typed));
        return Buffer.concat([length, typed, crc]);
    };
    const header = Buffer.alloc(13);
    header.writeUInt32BE(width, 0);
    header.writeUInt32BE(height, 4);
    header[8] = 8;
    header[9] = channels === 4 ? 6 : 2;
    return Buffer.concat([
        Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
        chunk('IHDR', header),
        chunk('IDAT', zlib.deflateSync(raw)),
        chunk('IEND', Buffer.alloc(0)),
    ]);
}

// ── GIF ──────────────────────────────────────────────────────────────────────

// GIF's LZW: codes from minSize + 1 bits, grown as the table grows, the table cleared when it is full.
function lzw(indexes, minSize) {
    const clear = 1 << minSize;
    const end = clear + 1;
    let size = minSize + 1;
    let next = end + 1;
    let table = new Map();
    const out = [];
    let acc = 0;
    let bits = 0;
    const emit = (code) => {
        acc |= code << bits;
        bits += size;
        while (bits >= 8) {
            out.push(acc & 0xff);
            acc >>>= 8;
            bits -= 8;
        }
    };
    emit(clear);
    let prefix = indexes[0];
    for (let i = 1; i < indexes.length; i++) {
        const key = prefix * 4096 + indexes[i];
        if (table.has(key)) {
            prefix = table.get(key);
            continue;
        }
        emit(prefix);
        if (next === 4096) {
            emit(clear);
            table = new Map();
            size = minSize + 1;
            next = end + 1;
        } else {
            if (next >= 1 << size) size++;
            table.set(key, next++);
        }
        prefix = indexes[i];
    }
    emit(prefix);
    emit(end);
    if (bits > 0) out.push(acc & 0xff);
    return Buffer.from(out);
}

const u16le = (v) => Buffer.from([v & 0xff, v >> 8]);

// A GIF89a with the 16 colors as its global palette; each frame { left, top, width, height, interlaced, transparent }
// draws the pattern's indexes where it lies.
function gif(frames) {
    const parts = [Buffer.from('GIF89a', 'latin1'), u16le(WIDTH), u16le(HEIGHT), Buffer.from([0xf3, 0, 0])];
    parts.push(Buffer.from(PALETTE.flat()));
    if (frames.length > 1) {
        parts.push(Buffer.from([0x21, 0xff, 11]), Buffer.from('NETSCAPE2.0', 'latin1'), Buffer.from([3, 1, 0, 0, 0]));
    }
    for (const frame of frames) {
        const { left = 0, top = 0, width = WIDTH, height = HEIGHT, interlaced = false, transparent = null } = frame;
        const packed = transparent === null ? 0 : 1;
        parts.push(Buffer.from([0x21, 0xf9, 4, packed]), u16le(10), Buffer.from([transparent ?? 0, 0]));
        parts.push(Buffer.from([0x2c]), u16le(left), u16le(top), u16le(width), u16le(height));
        parts.push(Buffer.from([interlaced ? 0x40 : 0]));
        const rows = [];
        for (let y = 0; y < height; y++) rows.push(y);
        const order = interlaced
            ? [[0, 8], [4, 8], [2, 4], [1, 2]].flatMap(([start, step]) => rows.filter((y) => y >= start && (y - start) % step === 0))
            : rows;
        const indexes = order.flatMap((y) => Array.from({ length: width }, (_, x) => index(left + x, top + y)));
        const data = lzw(indexes, 4);
        parts.push(Buffer.from([4]));
        for (let i = 0; i < data.length; i += 255) {
            const block = data.subarray(i, i + 255);
            parts.push(Buffer.from([block.length]), block);
        }
        parts.push(Buffer.from([0]));
    }
    parts.push(Buffer.from([0x3b]));
    return Buffer.concat(parts);
}

// ── TIFF ─────────────────────────────────────────────────────────────────────

const SHORT = 3;
const LONG = 4;
const RATIONAL = 5;

// An uncompressed TIFF of `images`, each { width, height, bits, photometric, data, tags } in one strip, little-endian
// unless `bigEndian`. `tags` adds or replaces tags, by number, as [type, values].
function tiff(images, bigEndian = false) {
    const u16 = (v) => {
        const b = Buffer.alloc(2);
        bigEndian ? b.writeUInt16BE(v) : b.writeUInt16LE(v);
        return b;
    };
    const u32 = (v) => {
        const b = Buffer.alloc(4);
        bigEndian ? b.writeUInt32BE(v) : b.writeUInt32LE(v);
        return b;
    };
    const chunks = [];
    let size = 0;
    const push = (bytes) => {
        const start = size;
        chunks.push(bytes);
        size += bytes.length;
        if (size % 2) {
            chunks.push(Buffer.alloc(1));
            size += 1;
        }
        return start;
    };
    push(Buffer.concat([Buffer.from(bigEndian ? 'MM' : 'II', 'latin1'), u16(42), u32(0)]));
    const links = [];
    let link = 4;
    for (const image of images) {
        const dataAt = push(image.data);
        const tags = {
            256: [LONG, [image.width]],
            257: [LONG, [image.height]],
            258: [SHORT, image.bits],
            259: [SHORT, [1]],
            262: [SHORT, [image.photometric]],
            273: [LONG, [dataAt]],
            277: [SHORT, [image.bits.length]],
            278: [LONG, [image.height]],
            279: [LONG, [image.data.length]],
            282: [RATIONAL, [72, 1]],
            283: [RATIONAL, [72, 1]],
            284: [SHORT, [1]],
            296: [SHORT, [2]],
            ...image.tags,
        };
        const entries = Object.entries(tags)
            .map(([tag, [type, values]]) => ({
                tag: Number(tag),
                type,
                count: type === RATIONAL ? values.length / 2 : values.length,
                bytes: Buffer.concat(values.map((v) => (type === SHORT ? u16(v) : u32(v)))),
            }))
            .sort((a, b) => a.tag - b.tag);
        for (const entry of entries) {
            if (entry.bytes.length > 4) entry.at = push(entry.bytes);
        }
        const ifd = [u16(entries.length)];
        for (const entry of entries) {
            const value = entry.bytes.length > 4 ? u32(entry.at) : Buffer.concat([entry.bytes, Buffer.alloc(4 - entry.bytes.length)]);
            ifd.push(u16(entry.tag), u16(entry.type), u32(entry.count), value);
        }
        ifd.push(u32(0));
        const ifdAt = push(Buffer.concat(ifd));
        links.push([link, ifdAt]);
        link = ifdAt + 2 + entries.length * 12;
    }
    const file = Buffer.concat(chunks);
    for (const [where, value] of links) {
        bigEndian ? file.writeUInt32BE(value, where) : file.writeUInt32LE(value, where);
    }
    return file;
}

// Rows of `width` samples of `bits` bits, each row starting on a byte.
function packed(width, height, bits, sample) {
    const stride = Math.ceil((width * bits) / 8);
    const out = Buffer.alloc(stride * height);
    for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
            const bit = x * bits;
            out[y * stride + (bit >> 3)] |= sample(x, y) << (8 - bits - (bit % 8));
        }
    }
    return out;
}

const rgbImage = { width: WIDTH, height: HEIGHT, bits: [8, 8, 8], photometric: 2, data: pixels(WIDTH, HEIGHT, rgb) };

// ── The files ────────────────────────────────────────────────────────────────

write('source.png', png(WIDTH, HEIGHT, 3, pixels(WIDTH, HEIGHT, rgb)));
write('source-alpha.png', png(WIDTH, HEIGHT, 4, pixels(WIDTH, HEIGHT, (x, y) => [...rgb(x, y), alpha(x)])));

run('cwebp', '-quiet', '-q', '90', at('source.png'), '-o', at('webp-lossy.webp'));
run('cwebp', '-quiet', '-q', '90', at('source-alpha.png'), '-o', at('webp-lossy-alpha.webp'));
run('cwebp', '-quiet', '-lossless', at('source-alpha.png'), '-o', at('webp-lossless-alpha.webp'));
run('img2webp', '-loop', '0', '-lossless', at('source.png'), at('source-alpha.png'), '-o', at('webp-animated.webp'));
// libwebp's own decode of the lossy files, which the tests compare the engine's with.
run('dwebp', '-quiet', at('webp-lossy.webp'), '-o', at('webp-lossy-libwebp.png'));
run('dwebp', '-quiet', at('webp-lossy-alpha.webp'), '-o', at('webp-lossy-alpha-libwebp.png'));

write('gif-opaque.gif', gif([{}]));
write('gif-interlaced.gif', gif([{ interlaced: true }]));
// Index 0, black, is transparent.
write('gif-transparent.gif', gif([{ transparent: 0 }]));
// A frame of 20 by 10 at (10, 5): the rest of the canvas is transparent.
write('gif-offset.gif', gif([{ left: 10, top: 5, width: 20, height: 10 }]));
write('gif-animated.gif', gif([{}, { transparent: 0 }]));
run('sips', '-s', 'format', 'gif', at('source.png'), '--out', at('gif-sips.gif'));

write('tiff-rgb.tif', tiff([rgbImage]));
run('tiffcp', '-c', 'lzw', at('tiff-rgb.tif'), at('tiff-rgb-lzw.tif'));
run('tiffcp', '-c', 'lzw:2', at('tiff-rgb.tif'), at('tiff-rgb-lzw-predictor.tif'));
run('tiffcp', '-c', 'zip', at('tiff-rgb.tif'), at('tiff-rgb-deflate.tif'));
run('tiffcp', '-c', 'packbits', at('tiff-rgb.tif'), at('tiff-rgb-packbits.tif'));
run('tiffcp', '-c', 'jpeg', at('tiff-rgb.tif'), at('tiff-jpeg.tif'));
write('tiff-rgba.tif', tiff([{
    width: WIDTH, height: HEIGHT, bits: [8, 8, 8, 8], photometric: 2,
    data: pixels(WIDTH, HEIGHT, (x, y) => [...rgb(x, y), alpha(x)]),
    tags: { 338: [SHORT, [2]] },
}]));
write('tiff-rgba-associated.tif', tiff([{
    width: WIDTH, height: HEIGHT, bits: [8, 8, 8, 8], photometric: 2,
    data: pixels(WIDTH, HEIGHT, (x, y) => [...rgb(x, y).map((c) => Math.round((c * alpha(x)) / 255)), alpha(x)]),
    tags: { 338: [SHORT, [1]] },
}]));
// 16-bit gray, big-endian: each pattern index as 17 times it in the high byte, so a file read in the wrong byte order
// comes out black.
write('tiff-gray16.tif', tiff([{
    width: WIDTH, height: HEIGHT, bits: [16], photometric: 1,
    data: pixels(WIDTH, HEIGHT, (x, y) => [index(x, y) * 17, 0]),
}], true));
write('tiff-palette4.tif', tiff([{
    width: WIDTH, height: HEIGHT, bits: [4], photometric: 3,
    data: packed(WIDTH, HEIGHT, 4, index),
    tags: { 320: [SHORT, [0, 1, 2].flatMap((c) => PALETTE.map((color) => color[c] * 257))] },
}]));
// 1 bit, white is zero as in a fax: an odd pattern index is black.
write('tiff-bilevel.tif', tiff([{
    width: WIDTH, height: HEIGHT, bits: [1], photometric: 0,
    data: packed(WIDTH, HEIGHT, 1, (x, y) => index(x, y) % 2),
}]));
run('tiffcp', '-c', 'g3', at('tiff-bilevel.tif'), at('tiff-fax-g3.tif'));
run('tiffcp', '-c', 'g4', at('tiff-bilevel.tif'), at('tiff-fax-g4.tif'));
write('tiff-cmyk.tif', tiff([{
    width: WIDTH, height: HEIGHT, bits: [8, 8, 8, 8], photometric: 5,
    data: pixels(WIDTH, HEIGHT, () => [0, 255, 255, 0]),
}]));
write('tiff-pages.tif', tiff([rgbImage, rgbImage]));
// The second image is a thumbnail, a reduced-resolution copy, which is not a page.
write('tiff-thumbnail.tif', tiff([rgbImage, {
    width: 8, height: 6, bits: [8, 8, 8], photometric: 2, data: pixels(8, 6, () => [0, 0, 0]),
    tags: { 254: [LONG, [1]] },
}]));
run('sips', '-s', 'format', 'tiff', at('source-alpha.png'), '--out', at('tiff-sips-alpha.tif'));

run('sips', '-s', 'format', 'bmp', at('source.png'), '--out', at('bmp-sips.bmp'));
run('sips', '-s', 'format', 'bmp', at('source-alpha.png'), '--out', at('bmp-sips-alpha.bmp'));

rmSync(at('.DS_Store'), { force: true });
