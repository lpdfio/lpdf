// Checks that the four SDKs name the attributes of every element as lpdf.xsd does. It reads the attribute
// classes of each SDK, generated and hand-written, turns each property back into the XML name its
// language's case rule gives it (fontSize, font_size and FontSize are font-size) and compares the set with
// the schema's. A class that has a property the schema lacks, or lacks one the schema has, is listed.
//
// The generated classes cannot drift while `make check-sdk-api` passes; this check also covers the
// classes that are written by hand: the document, its sections, meta, assets and tokens.
//
// Usage: node scripts/check-sdk-parity.mjs                      the four SDKs under src/sdk; list the differences, exit 1 if any
//        node scripts/check-sdk-parity.mjs --sdk node=.          only the SDK in the given folder (an SDK repository's CI)

import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { LANGUAGES, sdkRepositories } from './lib/sdk-repositories.mjs';
import { loadSchema } from './lib/xsd.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const schema = loadSchema(path.join(root, 'schema/lpdf.xsd'));
const { repositories, selected } = sdkRepositories(root, process.argv);

const attributes = (element, where = 'global') =>
    schema.element(element, where).attributes.map((a) => a.name).filter((name) => !name.startsWith('data-'));

/** The attributes each class has to have, by class name. */
const EXPECTED = {
    DocumentAttr: [...attributes('document'), 'assets', 'tokens', 'meta'],
    SectionAttr: attributes('section'),
    DocumentMeta: attributes('meta'),
    DocumentAssets: ['fonts', 'images'],
    DocumentTokens: ['colors', 'space', 'grid', 'border', 'radius', 'width', 'text-size'],
    FontAttr: attributes('font'),
    ImageAttr: attributes('image'),
    RegionAttr: attributes('region', 'layout'),
    LayerAttr: attributes('layer', 'canvas'),
    CanvasTextAttr: attributes('text', 'primitives'),
    CanvasImgAttr: attributes('img', 'primitives'),
};
for (const element of ['stack', 'flank', 'split', 'cluster', 'grid', 'frame', 'link', 'divider', 'span', 'img',
    'barcode', 'text', 'td', 'thead', 'tr', 'table', 'field']) {
    EXPECTED[`${element[0].toUpperCase()}${element.slice(1)}Attr`] = attributes(element);
}
for (const element of ['rect', 'circle', 'ellipse', 'line', 'path']) {
    EXPECTED[`${element[0].toUpperCase()}${element.slice(1)}Attr`] = attributes(element, 'primitives');
}

// ── What each SDK has ─────────────────────────────────────────────────────────

function files(dir, extension) {
    return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
        const full = path.join(dir, entry.name);
        if (entry.isDirectory()) return /^(node_modules|bin|obj|__pycache__|vendor|test|tests|dist)$/.test(entry.name) ? [] : files(full, extension);
        return full.endsWith(extension) ? [full] : [];
    });
}

const read = (file) => readFileSync(file, 'utf8').replace(/\r\n/g, '\n');
const kebab = {
    pascal: (name) => name.replace(/([a-z0-9])([A-Z])/g, '$1-$2').toLowerCase(),
    camel: (name) => name.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`),
    snake: (name) => name.replace(/_/g, '-'),
};
const wanted = (name) => name in EXPECTED;

/** Splits a parameter list on its commas, leaving the ones inside generic arguments such as Dictionary<string, string>. */
function splitParameters(list) {
    const parts = [];
    let depth = 0;
    let current = '';
    for (const c of list) {
        if (c === '<') depth++;
        if (c === '>') depth--;
        if (c === ',' && depth === 0) { parts.push(current); current = ''; } else current += c;
    }
    return [...parts, current];
}

const found = { dotnet: {}, node: {}, python: {}, php: {} };

const scans = {
    dotnet() {
        for (const file of files(path.join(repositories.dotnet, 'src'), '.cs')) {
            for (const [, name, members] of read(file).matchAll(/public sealed record (\w+)\(([\s\S]*?)\)\s*[;{:]/g)) {
                if (wanted(name)) found.dotnet[name] = splitParameters(members).map((m) => m.trim().match(/(\w+)\s*(?:=|$)/)?.[1]).filter(Boolean).map(kebab.pascal);
            }
            for (const [, name, body] of read(file).matchAll(/public sealed record (\w+)\s*\{([\s\S]*?)\n\}/g)) {
                if (wanted(name)) found.dotnet[name] = [...body.matchAll(/public (?:required )?[\w<>?,\[\] ]+? (\w+) \{ get; init; \}/g)].map((m) => kebab.pascal(m[1]));
            }
        }
    },
    node() {
        for (const file of files(path.join(repositories.node, 'src'), '.ts')) {
            for (const [, name, body] of read(file).matchAll(/export interface (\w+)(?:\s+extends[^{]+)?\s*\{([\s\S]*?)\n\}/g)) {
                if (wanted(name)) found.node[name] = [...body.matchAll(/^\s+(\w+)\??\s*:/gm)].map((m) => kebab.camel(m[1]));
            }
        }
    },
    python() {
        for (const file of files(path.join(repositories.python, 'src/lpdf'), '.py')) {
            for (const [, name, body] of read(file).matchAll(/^class (\w+)[^\n]*:\n((?:(?:    [^\n]*)?\n)+)/gm)) {
                if (wanted(name)) found.python[name] = [...body.matchAll(/^    (\w+)\s*:\s*[^\n=]+/gm)].map((m) => m[1]).filter((n) => !n.startsWith('_')).map(kebab.snake);
            }
        }
    },
    php() {
        for (const file of files(path.join(repositories.php, 'src'), '.php')) {
            for (const [, name, parameters] of read(file).matchAll(/class (\w+)[\s\S]*?__construct\(([\s\S]*?)\)\s*\{/g)) {
                if (wanted(name)) found.php[name] = [...parameters.matchAll(/\$(\w+)/g)].map((m) => kebab.camel(m[1]));
            }
        }
    },
};
for (const sdk of selected) scans[sdk]();

// ── Compare ───────────────────────────────────────────────────────────────────

let problems = 0;
for (const sdk of selected) {
    const classes = found[sdk];
    for (const [name, expected] of Object.entries(EXPECTED)) {
        const actual = classes[name];
        if (!actual) {
            problems++;
            console.log(`${sdk}: ${name} is missing`);
            continue;
        }
        const missing = expected.filter((attribute) => !actual.includes(attribute));
        const extra = actual.filter((attribute) => !expected.includes(attribute));
        if (missing.length || extra.length) {
            problems++;
            console.log(`${sdk}: ${name}${missing.length ? ` lacks ${missing.join(', ')}` : ''}${extra.length ? ` has ${extra.join(', ')} that the schema does not` : ''}`);
        }
    }
}
const where = selected.length === LANGUAGES.length ? 'all four SDKs' : `the ${selected.join(' and ')} SDK`;
console.log(problems ? `${problems} differences` : `all ${Object.keys(EXPECTED).length} classes match the schema in ${where}`);
if (problems) process.exit(1);
