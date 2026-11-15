// Checks what `merge` writes with two readers that share no code with the core: qpdf, which checks the structure of
// each merged file, and pdf.js, Firefox's PDF viewer, which renders each page of a source and the page of the merged file
// that stands for it, as it prints one, and compares them.
//
//  - Every merged file passes `qpdf --check` with no warning, and holds no annotation.
//  - Each page of a merged file is as big as its source page is shown, and renders like it: the same picture, with the
//    layers that print and the annotations that print, each from its stored appearance.
//  - A blank side the core adds to a document of odd length is as big as the page before it, and shows nothing.
//
// The samples come from the core's own tests:
//   LPDF_MERGE_OUT=<folder> cargo test --manifest-path src/core/Cargo.toml --lib writes_merge_samples_for_the_external_check
//
// Usage: node scripts/check-merge.mjs <folder> <pdfjs folder> [--png <folder>]   list the failures, exit 1 if any. The pdfjs
//        folder holds node_modules with pdfjs-dist and its @napi-rs/canvas (`npm install --prefix <folder>
//        pdfjs-dist@5.6.205`), which `make check-merge` installs. With --png, each pair that differs is written there as a
//        source image and a merged one, for a look, and with --all every pair is.

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const args = process.argv.slice(2);
const pngAt = args.indexOf('--png');
const pngFolder = pngAt >= 0 ? args.splice(pngAt, 2)[1] : null;
// With --all, every pair is written, not only the pairs that differ.
const allAt = args.indexOf('--all');
const savesAll = allAt >= 0 && args.splice(allAt, 1).length === 1;
const [folder, pdfjsFolder] = args;
if (!folder || !pdfjsFolder) {
    console.error('usage: node scripts/check-merge.mjs <folder of samples> <folder with node_modules/pdfjs-dist> [--png <folder> [--all]]');
    process.exit(2);
}

const SCALE = 1;
/** How far a pixel may be from the source's in a channel and still count as the same: anti-aliasing moves the edges. */
const TOLERANCE = 48;
/** How much of a page may differ: a form XObject is clipped to its box, which moves an edge by a pixel. */
const DIFFERENT = 0.002;

const pdfjsRoot = path.resolve(pdfjsFolder, 'node_modules/pdfjs-dist');
const pdfjs = await import(pathToFileURL(path.join(pdfjsRoot, 'legacy/build/pdf.mjs')).href);
const failures = [];
let compared = 0;
/** How much each file's source pages draw, so that a check that renders nothing can't pass. */
const inked = new Map();

function open(file) {
    return pdfjs.getDocument({
        data: new Uint8Array(fs.readFileSync(file)),
        standardFontDataUrl: `${path.join(pdfjsRoot, 'standard_fonts')}${path.sep}`,
        verbosity: 0,
    }).promise;
}

/** A page as it prints, white where it draws nothing: its size and its pixels. */
async function render(doc, page) {
    const viewport = page.getViewport({ scale: SCALE });
    const { canvas, context } = doc.canvasFactory.create(Math.ceil(viewport.width), Math.ceil(viewport.height));
    context.fillStyle = '#ffffff';
    context.fillRect(0, 0, canvas.width, canvas.height);
    await page.render({ canvas, canvasContext: context, viewport, intent: 'print', annotationMode: pdfjs.AnnotationMode.ENABLE }).promise;
    return { canvas, width: canvas.width, height: canvas.height, pixels: context.getImageData(0, 0, canvas.width, canvas.height).data };
}

/** The share of pixels of `b` that are not those of `a`. */
function compare(a, b) {
    let different = 0;
    for (let at = 0; at < a.pixels.length; at += 4) {
        if ([0, 1, 2].some((c) => Math.abs(a.pixels[at + c] - b.pixels[at + c]) > TOLERANCE)) different++;
    }
    return different / (a.width * a.height);
}

/** How many pixels of a page are not white. */
function ink(image) {
    let drawn = 0;
    for (let at = 0; at < image.pixels.length; at += 4) {
        if (image.pixels[at] < 250 || image.pixels[at + 1] < 250 || image.pixels[at + 2] < 250) drawn++;
    }
    return drawn;
}

function save(name, sourceImage, mergedImage) {
    if (!pngFolder) return;
    fs.mkdirSync(pngFolder, { recursive: true });
    fs.writeFileSync(path.join(pngFolder, `${name}.source.png`), sourceImage.canvas.toBuffer('image/png'));
    fs.writeFileSync(path.join(pngFolder, `${name}.merged.png`), mergedImage.canvas.toBuffer('image/png'));
}

function qpdfCheck(file) {
    try {
        execFileSync('qpdf', ['--check', file], { stdio: 'pipe' });
        return null;
    } catch (error) {
        return `${error.stdout ?? ''}${error.stderr ?? ''}`.trim().split('\n').slice(-3).join(' | ');
    }
}

/** Compare the page `at` of a merged file with page `number` of a source, or with nothing: a blank side. */
async function pair(label, merged, mergedPage, source, sourcePage, sizeOfBlank) {
    const page = await merged.getPage(mergedPage);
    const shown = await render(merged, page);
    if ((await page.getAnnotations()).length > 0) failures.push(`${label}: the merged page holds an annotation`);
    compared++;
    if (!source) {
        if (ink(shown) > 0) failures.push(`${label}: a blank side shows something`);
        const [w, h] = sizeOfBlank;
        if (Math.abs(shown.width - w) > 1 || Math.abs(shown.height - h) > 1) {
            failures.push(`${label}: the blank side is ${shown.width} by ${shown.height}, the page before it ${w} by ${h}`);
        }
        return [shown.width, shown.height];
    }
    const original = await render(source, await source.getPage(sourcePage));
    inked.set(label.split(' page ')[0], (inked.get(label.split(' page ')[0]) ?? 0) + ink(original));
    if (Math.abs(original.width - shown.width) > 1 || Math.abs(original.height - shown.height) > 1) {
        failures.push(`${label}: ${shown.width} by ${shown.height}, its source page is ${original.width} by ${original.height}`);
        return [shown.width, shown.height];
    }
    const different = compare(original, shown);
    if (different > DIFFERENT) failures.push(`${label}: ${(different * 100).toFixed(2)}% of the page differs from its source`);
    if (different > DIFFERENT || savesAll) save(label.replace(/[^A-Za-z0-9._-]/g, '_'), original, shown);
    return [shown.width, shown.height];
}

// Each file merged alone.
const names = fs.readdirSync(path.join(folder, 'sources')).filter((n) => n.endsWith('.pdf')).sort();
for (const name of names) {
    const mergedFile = path.join(folder, 'merged', name);
    const check = qpdfCheck(mergedFile);
    if (check) failures.push(`${name}: qpdf --check of the merged file: ${check}`);
    const source = await open(path.join(folder, 'sources', name));
    const merged = await open(mergedFile);
    if (merged.numPages !== source.numPages) {
        failures.push(`${name}: ${merged.numPages} pages merged, ${source.numPages} in the source`);
        continue;
    }
    for (let number = 1; number <= source.numPages; number++) {
        await pair(`${name} page ${number}`, merged, number, source, number);
    }
}

// All of them in one file, with a blank side after each document of odd length.
const all = path.join(folder, 'all.pdf');
for (const file of [all, path.join(folder, 'all-unlicensed.pdf')]) {
    const check = qpdfCheck(file);
    if (check) failures.push(`${path.basename(file)}: qpdf --check: ${check}`);
}
const index = JSON.parse(fs.readFileSync(path.join(folder, 'all.json'), 'utf8')).pages;
const together = await open(all);
if (together.numPages !== index.length) {
    failures.push(`all.pdf: ${together.numPages} pages, ${index.length} expected`);
} else {
    const sources = new Map();
    let before = [0, 0];
    for (let number = 1; number <= index.length; number++) {
        const entry = index[number - 1];
        if (entry.blank) {
            before = await pair(`all.pdf page ${number} (a blank side)`, together, number, null, 0, before);
            continue;
        }
        if (!sources.has(entry.source)) sources.set(entry.source, await open(path.join(folder, 'sources', entry.source)));
        before = await pair(`all.pdf page ${number} (${entry.source} page ${entry.page})`, together, number, sources.get(entry.source), entry.page);
    }
}

for (const name of names) {
    if (!(inked.get(name) > 0)) failures.push(`${name}: its source pages draw nothing, so the comparison shows nothing`);
}

if (failures.length > 0) {
    console.error(`${failures.length} failure(s):\n${failures.map((f) => `  ${f}`).join('\n')}`);
    process.exit(1);
}
console.log(`check-merge: ${names.length} files merged alone and together, ${compared} pages compared with pdf.js, qpdf --check clean`);
