// Opens the PDF/X files the core writes with qpdf, an implementation of the PDF standard that shares no code with ours,
// and checks what a printer's tools read from it:
//
//  - the file passes `qpdf --check`, and its header says PDF 1.6, which PDF/X-4 is a profile of;
//  - every page has a TrimBox inside its BleedBox, inside its MediaBox, and no annotation;
//  - RGB is mapped to an ICC profile on every page (`/DefaultRGB`);
//  - each spot color is a Separation space: the ink's name, the alternate device space of its stand-in, and a tint
//    transform that runs from white to the stand-in, with a component for each of the alternate's; a sample whose file
//    name has "spot" in it has at least one;
//  - the catalog holds one output intent, `GTS_PDFX`, with the press profile as an ICC stream of 4 components, and an
//    XMP metadata stream that names PDF/X-4;
//  - the Info dictionary names PDF/X-4 and that the file is not trapped, and the trailer has the file ID.
//
// No free tool validates PDF/X itself: Acrobat Pro's Preflight does, by hand, on the release candidate.
//
// The samples, the fixture, the brochure example, and the fixture with graph paper on its sections and with spot colors,
// come from the core's own tests:
//   LPDF_PDFX_OUT=<folder> cargo test --manifest-path src/core/Cargo.toml --lib writes_the_pdfx_sample_for_the_external_check
//
// Usage: node scripts/check-pdfx.mjs <folder>     list the failures, exit 1 if any
//        needs qpdf on the PATH (`apt install qpdf`, `brew install qpdf`)

import { spawnSync } from 'node:child_process';
import { readdirSync } from 'node:fs';
import path from 'node:path';

const folder = process.argv[2];
if (!folder) {
    console.error('usage: node scripts/check-pdfx.mjs <folder of samples>');
    process.exit(2);
}

const failures = [];

/** Runs qpdf; a non-zero exit is data here, not an error. */
function qpdf(...args) {
    const run = spawnSync('qpdf', args, { maxBuffer: 256 * 1024 * 1024, encoding: 'utf8' });
    if (run.error) {
        console.error(`could not run qpdf: ${run.error.message}`);
        process.exit(2);
    }
    return { status: run.status, stdout: run.stdout, output: `${run.stdout}${run.stderr}` };
}

function expect(ok, message) {
    if (!ok) failures.push(message);
}

/**
 * Checks that `space` is a spot color's Separation space, `[/Separation /ink /DeviceCMYK|/DeviceRGB function]`, with a
 * type 2 function from white to the stand-in, and calls `fail` with what is wrong.
 */
function checkSeparation(space, fail) {
    if (!Array.isArray(space) || space[0] !== '/Separation') {
        fail(`is ${JSON.stringify(space)}, not a Separation space`);
        return;
    }
    const [, ink, alternate, transform] = space;
    if (typeof ink !== 'string' || ink.length < 2 || ['/All', '/None', '/Cyan', '/Magenta', '/Yellow', '/Black'].includes(ink)) {
        fail(`names the ink ${ink}, which is not the name of a plate of its own`);
    }
    const white = { '/DeviceCMYK': [0, 0, 0, 0], '/DeviceRGB': [1, 1, 1] }[alternate];
    if (!white) {
        fail(`has the alternate ${alternate}, not /DeviceCMYK or /DeviceRGB`);
        return;
    }
    const components = (array) => Array.isArray(array) && array.length === white.length && array.every((c) => c >= 0 && c <= 1);
    if (transform?.['/FunctionType'] !== 2 || transform['/N'] !== 1) {
        fail(`has the tint transform ${JSON.stringify(transform)}, not a type 2 function of exponent 1`);
    } else if (JSON.stringify(transform['/C0']) !== JSON.stringify(white) || !components(transform['/C1'])) {
        fail(`runs from ${JSON.stringify(transform['/C0'])} to ${JSON.stringify(transform['/C1'])}, not from white to a color of ${alternate}`);
    }
}

/** Checks the PDF/X file `pdf`, and returns how many pages it has. */
function checkFile(pdf) {
    const check = (ok, message) => expect(ok, `${path.basename(pdf)}: ${message}`);
    const checked = qpdf('--check', pdf);
    check(checked.status === 0, `qpdf --check failed: ${checked.output.trim()}`);

    const json = qpdf('--json=2', '--json-key=qpdf', '--json-key=pages', '--json-stream-data=inline', '--decode-level=generalized', pdf);
    if (json.status !== 0) {
        check(false, `qpdf --json failed: ${json.output.trim()}`);
        return 0;
    }
    const { qpdf: [header, objects], pages } = JSON.parse(json.stdout);
    const object = (ref) => objects[`obj:${ref}`]?.value ?? objects[`obj:${ref}`]?.stream?.dict;
    const stream = (ref) => objects[`obj:${ref}`]?.stream;
    const resolve = (value) => (typeof value === 'string' && / \d+ R$/.test(value) ? object(value) : value);
    const text = (value) => (typeof value === 'string' ? value.replace(/^[ub]:/, '') : value);

    check(header.pdfversion === '1.6', `the header says PDF ${header.pdfversion}, not 1.6`);

    /** Whether box `inner` lies inside box `outer`, to a hundredth of a point. */
    const inside = (inner, outer) =>
        inner[0] >= outer[0] - 0.01 && inner[1] >= outer[1] - 0.01 && inner[2] <= outer[2] + 0.01 && inner[3] <= outer[3] + 0.01;

    check(pages.length > 0, 'the file has no page');
    let separations = 0;
    for (const page of pages) {
        const dict = object(page.object);
        const label = `page ${page.pageposfrom1}`;
        const [media, bleed, trim] = ['/MediaBox', '/BleedBox', '/TrimBox'].map((key) => dict[key]);
        check(Array.isArray(trim), `${label}: no TrimBox`);
        check(Array.isArray(bleed), `${label}: no BleedBox`);
        if (Array.isArray(trim) && Array.isArray(bleed) && Array.isArray(media)) {
            check(inside(trim, bleed) && inside(bleed, media), `${label}: the TrimBox ${trim} is not inside the BleedBox ${bleed}, inside the MediaBox ${media}`);
        }
        check(dict['/Annots'] === undefined, `${label}: it has annotations, which PDF/X allows none of on the printed page`);
        const resources = resolve(dict['/Resources']);
        const spaces = resolve(resources?.['/ColorSpace']);
        const defaultRgb = resolve(spaces?.['/DefaultRGB']);
        check(Array.isArray(defaultRgb) && defaultRgb[0] === '/ICCBased', `${label}: RGB is not mapped to an ICC profile with /DefaultRGB`);
        for (const [name, value] of Object.entries(spaces ?? {}).filter(([name]) => name !== '/DefaultRGB')) {
            separations += 1;
            checkSeparation(resolve(value), (message) => check(false, `${label}: the color space ${name} ${message}`));
        }
    }
    if (path.basename(pdf).includes('spot')) {
        check(separations > 0, 'no page names a Separation space, which the sample of spot colors draws with');
    }

    const catalog = object(objects.trailer.value['/Root']);
    const intents = resolve(catalog['/OutputIntents']) ?? [];
    check(intents.length === 1, `the catalog has ${intents.length} output intents, not one`);
    const intent = resolve(intents[0]) ?? {};
    check(intent['/S'] === '/GTS_PDFX', `the output intent is ${intent['/S']}, not /GTS_PDFX`);
    check(text(intent['/OutputConditionIdentifier'])?.length > 0, 'the output intent has no OutputConditionIdentifier');
    const profile = stream(intent['/DestOutputProfile']);
    check(profile?.dict['/N'] === 4, `the press profile's stream has /N ${profile?.dict['/N']}, not 4`);
    const profileBytes = profile?.data ? Buffer.from(profile.data, 'base64') : Buffer.alloc(0);
    check(profileBytes.subarray(36, 40).toString('latin1') === 'acsp' && profileBytes.subarray(12, 16).toString('latin1') === 'prtr',
        'the output intent\'s profile is not an ICC printer profile');

    const metadata = stream(catalog['/Metadata']);
    const xmp = metadata?.data ? Buffer.from(metadata.data, 'base64').toString('utf8') : '';
    check(xmp.includes('<pdfxid:GTS_PDFXVersion>PDF/X-4</pdfxid:GTS_PDFXVersion>'), 'the XMP packet does not name PDF/X-4');
    check(xmp.includes('<pdf:Trapped>False</pdf:Trapped>'), 'the XMP packet does not say the file is not trapped');

    const info = object(objects.trailer.value['/Info']);
    check(text(info['/GTS_PDFXVersion']) === 'PDF/X-4', `the Info dictionary's GTS_PDFXVersion is ${info['/GTS_PDFXVersion']}`);
    check(info['/Trapped'] === '/False', `the Info dictionary's Trapped is ${info['/Trapped']}, not /False`);
    check(info['/Title'] !== undefined, 'the Info dictionary has no Title');
    check(Array.isArray(objects.trailer.value['/ID']), 'the trailer has no file ID');
    return pages.length;
}

const files = readdirSync(folder).filter((name) => name.endsWith('.pdf')).sort();
expect(files.length > 0, `no PDF in ${folder}`);
const pages = files.reduce((count, name) => count + checkFile(path.join(folder, name)), 0);

if (failures.length > 0) {
    console.error(failures.map((failure) => `FAIL ${failure}`).join('\n'));
    process.exit(1);
}
console.log(`PDF/X check passed: qpdf reads ${files.join(', ')}, and their ${pages} pages, output intents, metadata and Info are what PDF/X-4 asks for`);
