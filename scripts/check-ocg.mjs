// Renders the samples of optional content the core writes with pdf.js, Firefox's PDF viewer, as Firefox shows a page
// and as it prints one, and checks what each shows by color: print content is red, screen content blue, and content in
// both groups at once, which never shows, green (`test/fixtures/showcase-ocg*.xml`).
//
//  - A sample that switches its groups must show blue and no red on screen, red and no blue in print, and green in
//    neither, on every page it names: a page that shows print content on screen means a part of a group, split
//    across pages or repeated on each, was drawn out of it.
//  - A sample of PDF/A or PDF/UA must show both red and blue on screen and in print: those files show everything.
//  - A sample of PDF/X must show red and no blue on screen and in print: it is the print version, with the screen
//    group in the file and off.
//
// pdf.js reads each group's print and view usage for the intent, as Acrobat applies them through the /AS events; it
// shares no code with the core.
//
// The samples come from the core's own tests:
//   LPDF_OCG_OUT=<folder> cargo test --manifest-path src/core/Cargo.toml --lib writes_ocg_samples_for_the_external_check
//
// Usage: node scripts/check-ocg.mjs <folder> <pdfjs folder>    list the failures, exit 1 if any. The pdfjs folder holds
//        node_modules with pdfjs-dist and its @napi-rs/canvas (`npm install --prefix <folder> pdfjs-dist@5.6.205`),
//        which `make check-ocg` installs.

import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const [folder, pdfjsFolder] = process.argv.slice(2);
if (!folder || !pdfjsFolder) {
    console.error('usage: node scripts/check-ocg.mjs <folder of samples> <folder with node_modules/pdfjs-dist>');
    process.exit(2);
}

/** The colors of the fixtures, as RGB. */
const COLORS = {
    print: [0xc0, 0x00, 0x00],
    screen: [0x00, 0x47, 0xab],
    never: [0x00, 0x8a, 0x00],
};
/** How far a pixel may be from a color in each channel and still count as it: anti-aliasing blends the edges. */
const TOLERANCE = 24;
/** How many pixels of a color make it shown: a glyph of 8pt text has more than this at the scale rendered. */
const SHOWN = 12;
const SCALE = 1.5;

/**
 * The samples `writes_ocg_samples_for_the_external_check` writes, and what each shows. `switches`: the groups switch,
 * on every page. `everything`: the file shows both groups, somewhere. `print`: the file shows the print version, on
 * screen and in print, somewhere.
 */
const SAMPLES = [
    { file: 'ocg.pdf', shows: 'switches' },
    { file: 'ocg-split.pdf', shows: 'switches', pages: 3 },
    { file: 'ocg-pdfa-2u.pdf', shows: 'everything' },
    { file: 'ocg-pdfua-1.pdf', shows: 'everything' },
    { file: 'ocg-pdfx.pdf', shows: 'print' },
];

const pdfjsRoot = path.resolve(pdfjsFolder, 'node_modules/pdfjs-dist');
const pdfjs = await import(pathToFileURL(path.join(pdfjsRoot, 'legacy/build/pdf.mjs')).href);
const version = JSON.parse(fs.readFileSync(path.join(pdfjsRoot, 'package.json'), 'utf8')).version;
const failures = [];

/** How many pixels of each color the page shows for `intent`, `display` or `print`. */
async function colorsOf(doc, page, intent) {
    const viewport = page.getViewport({ scale: SCALE });
    const { canvas, context } = doc.canvasFactory.create(Math.ceil(viewport.width), Math.ceil(viewport.height));
    context.fillStyle = '#ffffff';
    context.fillRect(0, 0, canvas.width, canvas.height);
    await page.render({ canvas, canvasContext: context, viewport, intent }).promise;
    const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
    const counts = Object.fromEntries(Object.keys(COLORS).map((name) => [name, 0]));
    for (let at = 0; at < pixels.length; at += 4) {
        for (const [name, [r, g, b]] of Object.entries(COLORS)) {
            if (Math.abs(pixels[at] - r) <= TOLERANCE && Math.abs(pixels[at + 1] - g) <= TOLERANCE && Math.abs(pixels[at + 2] - b) <= TOLERANCE) {
                counts[name]++;
            }
        }
    }
    return counts;
}

/** What a page of `sample` must show for `intent`, and must not. */
function expected(sample, intent) {
    if (sample.shows === 'everything') return { shown: ['print', 'screen'], hidden: ['never'] };
    if (sample.shows === 'print' || intent === 'print') return { shown: ['print'], hidden: ['screen', 'never'] };
    return { shown: ['screen'], hidden: ['print', 'never'] };
}

const checked = [];
for (const sample of SAMPLES) {
    const file = path.join(folder, sample.file);
    if (!fs.existsSync(file)) {
        failures.push(`${sample.file}: not written`);
        continue;
    }
    const doc = await pdfjs.getDocument({
        data: new Uint8Array(fs.readFileSync(file)),
        standardFontDataUrl: `${path.join(pdfjsRoot, 'standard_fonts')}${path.sep}`,
        verbosity: 0,
    }).promise;
    if (sample.pages && doc.numPages !== sample.pages) {
        failures.push(`${sample.file}: ${doc.numPages} pages, expected ${sample.pages}: the fixture no longer splits where it should`);
    }
    // A file that switches must show its colors on every page; any other, somewhere.
    const anywhere = { display: {}, print: {} };
    for (let number = 1; number <= doc.numPages; number++) {
        const page = await doc.getPage(number);
        for (const intent of ['display', 'print']) {
            const counts = await colorsOf(doc, page, intent);
            const { shown, hidden } = expected(sample, intent);
            const where = `${sample.file} page ${number}, ${intent === 'print' ? 'printed' : 'on screen'}`;
            for (const name of hidden) {
                if (counts[name] >= SHOWN) failures.push(`${where}: shows ${name} content (${counts[name]} pixels)`);
            }
            for (const name of shown) {
                anywhere[intent][name] = (anywhere[intent][name] ?? 0) + counts[name];
                if (sample.shows === 'switches' && counts[name] < SHOWN) failures.push(`${where}: shows no ${name} content`);
            }
        }
    }
    if (sample.shows !== 'switches') {
        for (const intent of ['display', 'print']) {
            for (const name of expected(sample, intent).shown) {
                if ((anywhere[intent][name] ?? 0) < SHOWN) {
                    failures.push(`${sample.file}, ${intent === 'print' ? 'printed' : 'on screen'}: shows no ${name} content`);
                }
            }
        }
    }
    checked.push(`${sample.file} (${doc.numPages} ${doc.numPages === 1 ? 'page' : 'pages'})`);
    await doc.destroy();
}

if (failures.length > 0) {
    console.error(failures.map((failure) => `FAIL ${failure}`).join('\n'));
    process.exit(1);
}
console.log(`Optional content check passed with pdf.js ${version}: ${checked.join(', ')}`);
