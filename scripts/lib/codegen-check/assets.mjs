// The files a document names, for the check of the code the code generator writes (scripts/check-codegen-sdk.mjs).
//
// A document declares the fonts, images, files and color profiles it needs, most with a `src` that the SDK would read
// from disk. The check loads each under the name the document gives it (its `ref`, or else its `name`), so the XML and
// the code render the same bytes and no SDK reads a `src` itself. It is what the docs' check does for the examples
// (`docs-site/scripts/lib/doc-xml.mjs` in the pages repository).
//
// The bytes are the file the `src` names, relative to the folder of the fixtures, when the checkout has it: the fixtures
// that name files in `test/assets` get their fonts, whose glyphs a PDF/A or PDF/X document needs, and their images,
// files and press profile. Otherwise they are placeholders:
//
//   fonts      the Radley of the test assets
//   images     a one-pixel PNG
//   files      a few placeholder bytes, or for the invoice of a Factur-X document (factur-x.xml, xrechnung.xml) the XML
//              of an invoice that names its profile, which the engine reads
//   profiles   the header of a CMYK printer profile, enough for a PDF/X document to take it as its press profile

import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';

/** The date of every render of the check, which a document that asks for PDF/A, PDF/UA or PDF/X needs. */
export const CREATED_ON = '2026-01-01T00:00:00Z';

const PIXEL = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==', 'base64');
const FILE = Buffer.from('Placeholder for a file a document attaches to a PDF.\n');

/** The XML of a Factur-X invoice whose BT-24 is `guideline`, as little of it as the engine reads. */
function invoice(guideline) {
    return Buffer.from(
        '<rsm:CrossIndustryInvoice xmlns:rsm="urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100"'
        + ' xmlns:ram="urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100">'
        + '<rsm:ExchangedDocumentContext><ram:GuidelineSpecifiedDocumentContextParameter>'
        + `<ram:ID>${guideline}</ram:ID>`
        + '</ram:GuidelineSpecifiedDocumentContextParameter></rsm:ExchangedDocumentContext></rsm:CrossIndustryInvoice>',
    );
}

/** The header of an ICC printer profile in CMYK, with a description and nothing else. */
function profile() {
    const description = 'Placeholder press';
    const desc = Buffer.alloc(12 + description.length + 1 + 12);
    desc.write('desc', 0, 'latin1');
    desc.writeUInt32BE(description.length + 1, 8);
    desc.write(description, 12, 'latin1');
    const header = Buffer.alloc(128);
    header.write('prtrCMYKLab ', 12, 'latin1');
    header.write('acsp', 36, 'latin1');
    const table = Buffer.alloc(16);
    table.writeUInt32BE(1, 0);
    table.write('desc', 4, 'latin1');
    table.writeUInt32BE(144, 8);
    table.writeUInt32BE(desc.length, 12);
    const bytes = Buffer.concat([header, table, desc]);
    bytes.writeUInt32BE(bytes.length, 0);
    return bytes;
}

/** The file name of each placeholder in the folder `writePlaceholders` fills, by what it stands for. */
const FILES = {
    font: 'font.ttf',
    image: 'image.png',
    file: 'file.txt',
    'factur-x.xml': 'factur-x.xml',
    'xrechnung.xml': 'xrechnung.xml',
    profile: 'profile.icc',
};

/**
 * Writes the placeholders into `dir`, the font from `test/assets/fonts` of the core checkout `root`. Returns the path of
 * each, by what it stands for.
 */
export function writePlaceholders(root, dir) {
    mkdirSync(dir, { recursive: true });
    const bytes = {
        font: readFileSync(path.join(root, 'test/assets/fonts/Radley-Regular.ttf')),
        image: PIXEL,
        file: FILE,
        'factur-x.xml': invoice('urn:cen.eu:en16931:2017'),
        'xrechnung.xml': invoice('urn:cen.eu:en16931:2017#compliant#urn:xeinkauf.de:kosit:xrechnung_3.0'),
        profile: profile(),
    };
    const paths = {};
    for (const [what, file] of Object.entries(FILES)) {
        paths[what] = path.join(dir, file);
        writeFileSync(paths[what], bytes[what]);
    }
    return paths;
}

/** The value of an attribute in the text of a start tag, or undefined. A `data-ref` is not a `ref`. */
function attribute(tag, name) {
    return new RegExp(`(?<![\\w-])${name}=(?:"([^"]*)"|'([^']*)')`).exec(tag)?.slice(1).find((value) => value !== undefined);
}

/**
 * What `xml` declares, in the order it declares it: `{ kind, key, path }` for each font, image, file and color
 * profile, with the key to load it under (its `ref`, or else its `name`) and the file to load there: the one its `src`
 * names, relative to `base`, when it is there, else the placeholder in `placeholders`.
 */
export function declaredAssets(xml, base, placeholders) {
    const declared = [];
    for (const kind of ['font', 'image', 'file', 'profile']) {
        for (const [start] of xml.matchAll(new RegExp(`<${kind}\\b[^>]*>`, 'g'))) {
            const name = attribute(start, 'name');
            const key = attribute(start, 'ref') ?? name;
            if (key === undefined) continue;
            const src = attribute(start, 'src');
            const named = src === undefined ? undefined : path.resolve(base, src);
            declared.push({ kind, key, path: named && existsSync(named) ? named : placeholders[name] ?? placeholders[kind] });
        }
    }
    return declared;
}
