#!/usr/bin/env node
// Copies the examples in examples/ to the three places that show them, so there is one source:
//
//   the extension   src/vscode/templates/<id>/    what Lpdf: New Document offers
//   the demo        <pages>/ui/demo/examples/     the examples of the demo on lpdf.io, with index.json
//   the docs        <pages>/www/docs/content/examples/<id>.md, and <pages>/docs-site/public/examples/<id>/
//                   a page for each example, and the files it links to
//
// An example is a folder examples/<n>-<id>/ that holds:
//
//   example.json    { "label", "description", "fileName" }   what the lists show, and the name a new document is saved under
//   document.xml    the document, with a comment at the top that says what it shows
//   document.json   its data, if it has any
//   assets/         the fonts and images it names; only those, and the licence and readme files beside them, are copied
//
// <n> orders the list. The other files of a folder, such as the script that generates the book, are not copied.
// Adding an example is adding a folder and running this; nothing else lists them.
//
// What this writes, it owns: a file in a place below that this does not produce is deleted. The extension's
// templates are the exception: only a template folder with a .synced-from file is touched, so a template written by
// hand stays.
//
// Usage: node scripts/sync-examples.mjs            write the files
//        node scripts/sync-examples.mjs --check    list the files that are not as this would write them, exit 1 if any
//
// The pages checkout is found at ../codesense/pages/lpdf, as the Makefile does, or at $PAGES_DIR or --pages <dir>.
// Without one, only the extension is synced.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '..');
const examplesDir = path.join(repo, 'examples');

const args = process.argv.slice(2);
const check = args.includes('--check');
const pagesArg = args.includes('--pages') ? args[args.indexOf('--pages') + 1] : undefined;
const pagesDir = path.resolve(pagesArg ?? process.env.PAGES_DIR ?? path.join(repo, '..', 'codesense', 'pages', 'lpdf'));

/** A document of this many lines or fewer is shown whole in its docs page; a longer one is linked. */
const INLINE_XML_MAX_LINES = 260;

const MARKER = '.synced-from';
const LICENCE_FILE = /^(OFL.*|LICEN[SC]E.*|README.*|NOTICE.*)$/i;

// ── Reading the examples ──────────────────────────────────────────────────────

/** @returns {Example[]} the examples in the order of their folder names */
function readExamples() {
    const examples = [];
    for (const entry of fs.readdirSync(examplesDir, { withFileTypes: true })) {
        const match = /^(\d+)-(.+)$/.exec(entry.name);
        if (!entry.isDirectory() || !match) continue;
        const folder = path.join(examplesDir, entry.name);
        const manifestPath = path.join(folder, 'example.json');
        if (!fs.existsSync(manifestPath)) fail(`${entry.name}: example.json is missing`);
        const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
        for (const key of ['label', 'description', 'fileName']) {
            if (typeof manifest[key] !== 'string' || !manifest[key].trim()) fail(`${entry.name}: example.json needs "${key}" as text`);
        }
        const xmlPath = path.join(folder, 'document.xml');
        if (!fs.existsSync(xmlPath)) fail(`${entry.name}: document.xml is missing`);
        const xml = fs.readFileSync(xmlPath, 'utf8');
        const dataPath = path.join(folder, 'document.json');
        const data = fs.existsSync(dataPath) ? fs.readFileSync(dataPath) : undefined;
        if (data) JSON.parse(data.toString('utf8'));
        examples.push({
            id: match[2],
            order: Number(match[1]),
            folderName: entry.name,
            label: manifest.label,
            description: manifest.description,
            fileName: manifest.fileName,
            xml: Buffer.from(xml, 'utf8'),
            xmlText: xml,
            data,
            assets: assetFiles(folder, xml),
        });
    }
    examples.sort((a, b) => a.order - b.order);
    return examples;
}

/**
 * The files an example needs next to its document: each one its XML names with src, and the licence and
 * readme files in the same folders. Paths are relative to the example, with / between folders.
 */
function assetFiles(folder, xml) {
    const wanted = new Set();
    // a src inside a comment is an example in the text, not a file the document needs
    for (const match of xml.replace(/<!--[\s\S]*?-->/g, '').matchAll(/\bsrc="([^"]+)"/g)) {
        const rel = match[1];
        if (/^[a-z]+:/i.test(rel) || rel.startsWith('/')) continue;
        const file = path.join(folder, rel);
        if (!fs.existsSync(file)) fail(`${path.basename(folder)}: the XML names ${rel}, which is not there`);
        wanted.add(rel.replace(/\\/g, '/'));
        const dir = path.dirname(file);
        for (const name of fs.readdirSync(dir)) {
            if (LICENCE_FILE.test(name)) wanted.add(path.relative(folder, path.join(dir, name)).replace(/\\/g, '/'));
        }
    }
    return [...wanted].sort().map(rel => ({ rel, bytes: fs.readFileSync(path.join(folder, rel)) }));
}

function fail(message) {
    console.error(`sync-examples: ${message}`);
    process.exit(2);
}

// ── What each target gets ─────────────────────────────────────────────────────

/** Files in the order of a template: the XML, its data, its assets. */
function exampleFiles(example, prefix) {
    const files = new Map();
    files.set(`${prefix}document.xml`, example.xml);
    if (example.data) files.set(`${prefix}document.json`, example.data);
    for (const asset of example.assets) files.set(`${prefix}${asset.rel}`, asset.bytes);
    return files;
}

function json(value) {
    return Buffer.from(`${JSON.stringify(value, null, 2)}\n`, 'utf8');
}

function extensionTarget(examples) {
    const root = path.join(repo, 'src', 'vscode', 'templates');
    const files = new Map();
    for (const example of examples) {
        const prefix = `${example.id}/`;
        for (const [rel, bytes] of exampleFiles(example, prefix)) files.set(rel, bytes);
        files.set(`${prefix}template.json`, json({
            label: example.label,
            description: example.description,
            fileName: example.fileName,
            order: example.order,
        }));
        files.set(`${prefix}${MARKER}`, Buffer.from(`examples/${example.folderName}\n`, 'utf8'));
    }
    // Only a template folder this has written is its to prune.
    const owns = rel => {
        const top = rel.split('/')[0];
        return fs.existsSync(path.join(root, top, MARKER));
    };
    return { name: 'the extension', root, files, owns, protectUnmarked: true };
}

function demoTarget(examples) {
    const root = path.join(pagesDir, 'ui', 'demo', 'examples');
    const files = new Map();
    for (const example of examples) {
        for (const [rel, bytes] of exampleFiles(example, `${example.id}/`)) files.set(rel, bytes);
    }
    files.set('index.json', json({
        examples: examples.map(example => ({
            id: example.id,
            label: example.label,
            description: example.description,
            xml: `${example.id}/document.xml`,
            data: example.data ? `${example.id}/document.json` : null,
        })),
    }));
    return { name: 'the demo', root, files, owns: () => true };
}

/** The first comment of a document: what it says it shows, without the indent. */
function headerNotes(xml) {
    const match = /<!--([\s\S]*?)-->/.exec(xml.slice(xml.indexOf('<lpdf')));
    if (!match) return '';
    const lines = match[1].replace(/\r/g, '').split('\n');
    while (lines.length && !lines[0].trim()) lines.shift();
    while (lines.length && !lines[lines.length - 1].trim()) lines.pop();
    const indent = Math.min(...lines.filter(line => line.trim()).map(line => /^ */.exec(line)[0].length));
    return lines.map(line => line.slice(indent).trimEnd()).join('\n');
}

function docsPage(example) {
    const lineCount = example.xmlText.split('\n').length;
    const links = [
        `[Open it in the demo](/?example=${example.id})`,
        `[document.xml](/docs/examples/${example.id}/document.xml)`,
    ];
    if (example.data) links.push(`[document.json](/docs/examples/${example.id}/document.json)`);
    const notes = headerNotes(example.xmlText);
    const parts = [
        `# ${example.label}`,
        '',
        example.description,
        '',
        links.join(' · '),
        '',
    ];
    // The PDF is rendered by the docs build (scripts/render-examples.mjs of docs-site) from the files written below, and
    // shown in the same viewer as the demo and the extension, which is a page of lpdf.io that loads the PDF it is given.
    parts.push(
        '<iframe class="lpdf-example-viewer"',
        `        src="/assets/js/lpdf-demo/viewer/index.html?pdf=/docs/examples/${example.id}/document.pdf"`,
        `        title="The ${example.label.toLowerCase()}, in the PDF viewer"`,
        '        loading="lazy"></iframe>',
        '',
    );
    if (notes) parts.push('## What it shows', '', '```text', notes, '```', '');
    const extra = example.assets.filter(asset => !LICENCE_FILE.test(path.basename(asset.rel)));
    if (extra.length) {
        parts.push(
            '## Fonts and images',
            '',
            `The document names ${extra.length} file${extra.length === 1 ? '' : 's'}, which go in an \`assets\` folder next to it. ` +
            'In Visual Studio Code, **Lpdf: New Document** puts them there.',
            '',
            ...extra.map(asset => `- [${asset.rel}](/docs/examples/${example.id}/${asset.rel})`),
            '',
        );
    }
    if (lineCount <= INLINE_XML_MAX_LINES) {
        parts.push('## The document', '', '```xml', example.xmlText.trimEnd(), '```', '');
        if (example.data) parts.push('## The data', '', '```json', example.data.toString('utf8').trimEnd(), '```', '');
    } else {
        parts.push(
            '## The document',
            '',
            `The document is ${lineCount.toLocaleString('en-US')} lines, so it is not repeated here. ` +
            'Open it in the demo to read it beside the PDF it makes, or download it above.',
            '',
        );
    }
    return Buffer.from(parts.join('\n'), 'utf8');
}

function docsTargets(examples) {
    const pages = new Map();
    for (const example of examples) pages.set(`${example.id}.md`, docsPage(example));
    pages.set('index.json', json({ examples: examples.map(example => ({ id: example.id, label: example.label })) }));
    const downloads = new Map();
    for (const example of examples) {
        for (const [rel, bytes] of exampleFiles(example, `${example.id}/`)) downloads.set(rel, bytes);
    }
    return [
        // index.json is read by docs-site/astro.config.mjs for the sidebar; the content sync reads only the .md files.
        { name: 'the docs pages', root: path.join(pagesDir, 'www', 'docs', 'content', 'examples'), files: pages, owns: () => true },
        // document.pdf is rendered by the docs build, not written here, and is not in git: it is not this script's to remove.
        { name: 'the docs downloads', root: path.join(pagesDir, 'docs-site', 'public', 'examples'), files: downloads, owns: rel => !rel.endsWith('/document.pdf') },
    ];
}

// ── Writing, or checking ──────────────────────────────────────────────────────

function listFiles(root, rel = '') {
    const dir = path.join(root, rel);
    if (!fs.existsSync(dir)) return [];
    return fs.readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
        const r = rel ? `${rel}/${entry.name}` : entry.name;
        return entry.isDirectory() ? listFiles(root, r) : [r];
    });
}

function removeEmptyFolders(root, rel = '') {
    const dir = path.join(root, rel);
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
        if (entry.isDirectory()) removeEmptyFolders(root, rel ? `${rel}/${entry.name}` : entry.name);
    }
    if (rel && fs.readdirSync(dir).length === 0) fs.rmdirSync(dir);
}

/** @returns {number} how many files differ from what this would write */
function apply(target) {
    const { root, files, owns, name } = target;
    let changes = 0;
    const note = (what, rel) => { changes++; console.log(`  ${what} ${path.relative(repo, path.join(root, rel)).replace(/\\/g, '/')}`); };

    // A template folder that this did not write is not its to overwrite.
    if (target.protectUnmarked) {
        for (const top of new Set([...files.keys()].map(rel => rel.split('/')[0]))) {
            const topDir = path.join(root, top);
            if (fs.existsSync(topDir) && !fs.existsSync(path.join(topDir, MARKER))) {
                fail(`${path.relative(repo, topDir)} exists and was not written by this script; remove it, or rename the example`);
            }
        }
    }

    for (const [rel, bytes] of files) {
        const file = path.join(root, rel);
        const present = fs.existsSync(file) ? fs.readFileSync(file) : undefined;
        if (present && present.equals(bytes)) continue;
        note(present ? 'update' : 'add   ', rel);
        if (!check) {
            fs.mkdirSync(path.dirname(file), { recursive: true });
            fs.writeFileSync(file, bytes);
        }
    }
    for (const rel of listFiles(root)) {
        if (files.has(rel) || !owns(rel)) continue;
        note('remove', rel);
        if (!check) fs.rmSync(path.join(root, rel));
    }
    if (!check && fs.existsSync(root)) removeEmptyFolders(root);
    if (changes === 0) console.log(`${name}: up to date`);
    return changes;
}

// ── Main ──────────────────────────────────────────────────────────────────────

const examples = readExamples();
if (examples.length === 0) fail(`no examples in ${examplesDir}`);
const targets = [extensionTarget(examples)];
if (fs.existsSync(path.join(pagesDir, 'www'))) {
    targets.push(demoTarget(examples), ...docsTargets(examples));
} else {
    console.log(`The pages checkout is not at ${pagesDir}: the demo and the docs are skipped.`);
}

let total = 0;
for (const target of targets) {
    console.log(`${check ? 'Checking' : 'Syncing'} ${target.name} (${examples.length} examples)`);
    total += apply(target);
}
if (check && total > 0) {
    console.error(`\n${total} file${total === 1 ? '' : 's'} differ. Run: node scripts/sync-examples.mjs`);
    process.exit(1);
}
