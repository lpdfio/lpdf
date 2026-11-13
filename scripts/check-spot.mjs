// Renders the plates of the page of spot colors the core writes with Ghostscript's tiffsep device, an implementation of
// PDF that shares no code with ours, and reads what each plate prints, as a press would:
//
//  - the page has the four process plates and one for each ink, named after it, and no other;
//  - the solid brand color prints 100% of its ink on its own plate, and nothing on the process plates, which its CMYK
//    stand-in would have printed on;
//  - its 30% tint prints 30% of the ink;
//  - under the varnish, which overprints, the cyan of the square below it still prints, and the varnish too;
//  - under the knockout ink, which doesn't overprint, the cyan plate is empty, and the knockout ink prints;
//  - the gradient of the brand color runs from 100% of the ink at its left to 0% at its right;
//  - where the page has nothing, no plate prints.
//
// The page is 200 by 100pt, rendered at 72 dpi, a pixel a point, and each place is read in the middle of its square
// (`plates_sample` in `spot_render_tests.rs`). A plate's file is 8-bit gray, black where the ink prints in full.
//
// The sample comes from the core's own tests:
//   LPDF_SPOT_OUT=<folder> cargo test --manifest-path src/core/Cargo.toml --lib writes_the_spot_sample_for_the_external_check
//
// Usage: node scripts/check-spot.mjs <folder>     list the failures, exit 1 if any
//        needs Docker and the image of scripts/check-spot.Dockerfile, tagged lpdf-check-spot (`make check-spot` builds it)

import { spawnSync } from 'node:child_process';
import { mkdirSync, readdirSync, readFileSync, rmSync } from 'node:fs';
import path from 'node:path';

const IMAGE = 'lpdf-check-spot';
// How far a reading may be from what is printed, in percent of the ink: Ghostscript rounds a tint to 1 of 255.
const TOLERANCE = 2;

const folder = process.argv[2];
if (!folder) {
    console.error('usage: node scripts/check-spot.mjs <folder of the sample>');
    process.exit(2);
}

const failures = [];
const expect = (ok, message) => {
    if (!ok) failures.push(message);
};

/** Renders each plate of `plates.pdf` in `folder` to `plates/page1(<ink>).tif`, with the composite as `page1.tif`. */
function renderPlates() {
    const out = path.join(folder, 'plates');
    rmSync(out, { recursive: true, force: true });
    mkdirSync(out, { recursive: true });
    const run = spawnSync('docker', [
        'run', '--rm', '-v', `${path.resolve(folder)}:/work`, IMAGE,
        '-q', '-dNOPAUSE', '-dBATCH', '-dSAFER', '-sDEVICE=tiffsep', '-r72', '-sCompression=none',
        '-o', '/work/plates/page%d.tif', '/work/plates.pdf',
    ], { encoding: 'utf8' });
    if (run.error || run.status !== 0) {
        console.error(`could not render the plates with Ghostscript: ${run.error?.message ?? `${run.stdout}${run.stderr}`.trim()}`);
        process.exit(2);
    }
    return out;
}

/** An uncompressed 8-bit gray TIFF, as tiffsep writes a plate: how much ink it prints at (x, y), in percent. */
function readPlate(file) {
    const bytes = readFileSync(file);
    const little = bytes.toString('latin1', 0, 2) === 'II';
    const u16 = (at) => (little ? bytes.readUInt16LE(at) : bytes.readUInt16BE(at));
    const u32 = (at) => (little ? bytes.readUInt32LE(at) : bytes.readUInt32BE(at));
    const ifd = u32(4);
    const tags = new Map();
    for (let i = 0; i < u16(ifd); i += 1) {
        const entry = ifd + 2 + i * 12;
        const [type, count] = [u16(entry + 2), u32(entry + 4)];
        // A value of a short or a long, or the values it points to when they don't fit in its 4 bytes.
        const size = type === 3 ? 2 : 4;
        const at = count * size <= 4 ? entry + 8 : u32(entry + 8);
        const values = Array.from({ length: count }, (_, n) => (size === 2 ? u16(at + n * 2) : u32(at + n * 4)));
        tags.set(u16(entry), values);
    }
    const value = (tag) => tags.get(tag)?.[0];
    const [width, height, bits, compression, photometric] = [value(256), value(257), value(258), value(259), value(262)];
    if (bits !== 8 || compression !== 1 || photometric !== 1) {
        throw new Error(`${path.basename(file)}: not an uncompressed 8-bit gray TIFF`);
    }
    const strips = tags.get(273);
    const rows = value(278) ?? height;
    // Black is zero: 0 is the ink in full, 255 none of it.
    return (x, y) => ((255 - bytes[strips[Math.floor(y / rows)] + (y % rows) * width + x]) / 255) * 100;
}

const PROCESS = ['Cyan', 'Magenta', 'Yellow', 'Black'];
const INKS = ['PANTONE 1665 C', 'Varnish', 'Knockout'];

const plates = renderPlates();
const found = new Map(
    readdirSync(plates)
        .map((name) => [name.match(/^page1\((.+)\)\.tif$/)?.[1], name])
        .filter(([ink]) => ink)
        .map(([ink, name]) => [ink, readPlate(path.join(plates, name))]),
);
const names = [...found.keys()].sort();
expect(
    JSON.stringify(names) === JSON.stringify([...PROCESS, ...INKS].sort()),
    `the plates are ${names.join(', ')}, not the four process plates and ${INKS.join(', ')}`,
);

/** Checks that `ink` prints `percent` at the middle (x, y) of what `place` names. */
function prints(ink, x, y, percent, place) {
    const plate = found.get(ink);
    if (!plate) return;
    const read = plate(x, y);
    expect(Math.abs(read - percent) <= TOLERANCE, `${place}: the ${ink} plate prints ${read.toFixed(1)}%, not ${percent}%`);
}

// The squares are 20pt from (x, 10) to (x + 20, 30), read at their middle, row 20.
for (const ink of [...PROCESS, ...INKS.slice(1)]) prints(ink, 20, 20, 0, 'the solid brand color');
prints('PANTONE 1665 C', 20, 20, 100, 'the solid brand color');
for (const ink of [...PROCESS, ...INKS.slice(1)]) prints(ink, 50, 20, 0, 'the 30% tint');
prints('PANTONE 1665 C', 50, 20, 30, 'the 30% tint');
prints('Cyan', 80, 20, 100, 'cyan under the varnish, which overprints');
prints('Varnish', 80, 20, 100, 'cyan under the varnish, which overprints');
prints('Cyan', 110, 20, 0, 'cyan under the knockout ink, which doesn\'t overprint');
prints('Knockout', 110, 20, 100, 'cyan under the knockout ink, which doesn\'t overprint');
for (const ink of [...PROCESS, ...INKS]) prints(ink, 150, 20, 0, 'the empty page');

// The gradient runs across (10, 50) to (190, 70), from 100% at its left edge to 0% at its right; read along row 60.
for (const x of [15, 55, 100, 145, 185]) {
    const percent = 100 * (1 - (x + 0.5 - 10) / 180);
    prints('PANTONE 1665 C', x, 60, Math.round(percent * 10) / 10, `the gradient at x ${x}`);
    prints('Magenta', x, 60, 0, `the gradient at x ${x}`);
}

if (failures.length > 0) {
    console.error(failures.map((failure) => `FAIL ${failure}`).join('\n'));
    process.exit(1);
}
console.log(
    `Spot check passed: Ghostscript's tiffsep prints each ink on its own plate (${INKS.join(', ')}), a tint at its `
    + 'percentage, a gradient of tints from 100% to 0%, and the process plates under an ink that overprints',
);
