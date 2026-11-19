#!/usr/bin/env node
// Copies Lpdf's default invoice map, src/core/assets/cross-industry-invoice-map.xml, to the docs site, and writes the
// page that lists its names from it, so there is one source:
//
//   the download    <pages>/www/schema/cross-industry-invoice-map.xml
//   the page        <pages>/www/docs/content/invoice-map.md
//
// The page has a section for each comment that stands alone at the top of the map (<!-- The seller -->), and a table
// of its entries: each one's full name, the comment beside it, which gives the number and the name EN 16931 has for
// it, and its type, and whether it is required. A comment "less common" starts a second table in the section.
//
// Usage: node scripts/sync-invoice-map.mjs            write the files
//        node scripts/sync-invoice-map.mjs --check    list the files that are not as this would write them, exit 1 if any
//
// The pages checkout is found at repos/lpdf-pages, as the Makefile does, or at $PAGES_DIR or --pages <dir>.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '..');
const MAP = path.join(repo, 'src/core/assets/cross-industry-invoice-map.xml');

/** An entry of the map: a value, a group or a list, with its full name, attributes and the comment beside it. */
function entry(tag, attributes, outer) {
    const name = attributes.name;
    return { tag, name, fullName: outer ? `${outer}.${name}` : name, type: attributes.type, required: attributes.required === 'true', note: '' };
}

const ATTRIBUTE = /([\w-]+)="([^"]*)"/g;

/**
 * The sections of the map: each with its heading, and its entries in two tables, the common ones and the less common
 * ones, in the order of the map. An entry inside a group or a list follows it, with its full name.
 */
export function readMap(text) {
    const sections = [];
    let section = null;
    let rows = null;
    const outer = [];
    let last = null;
    let lastLineEnd = -1;
    let inRoot = false;
    const token = /<!--([\s\S]*?)-->|<(\/?)(value|group|list|cross-industry-invoice-map)\b([^>]*?)(\/?)>/g;
    for (const match of text.matchAll(token)) {
        const [whole, comment, closing, tag, attributes, empty] = match;
        const lineOfStart = text.lastIndexOf('\n', match.index);
        if (comment !== undefined) {
            const note = comment.trim().replace(/\s+/g, ' ');
            if (last && lineOfStart === lastLineEnd) {
                // The comment beside an entry, on the line its tag ends on.
                last.note = note;
            } else if (inRoot && note === 'less common') {
                if (section) section.tables.push((rows = []));
            } else if (inRoot && outer.length === 0) {
                section = { heading: note, tables: [(rows = [])] };
                sections.push(section);
            }
            last = null;
            continue;
        }
        if (tag === 'cross-industry-invoice-map') {
            inRoot = !closing;
            continue;
        }
        if (closing) {
            outer.pop();
            last = null;
            continue;
        }
        const values = Object.fromEntries([...attributes.matchAll(ATTRIBUTE)].map(([, key, value]) => [key, value]));
        const item = entry(tag, values, outer.at(-1));
        if (!rows) throw new Error(`${item.fullName} comes before the first heading of the map`);
        rows.push(item);
        last = item;
        lastLineEnd = text.lastIndexOf('\n', match.index + whole.length);
        if (tag !== 'value' && !empty) outer.push(item.fullName);
    }
    return sections;
}

/** What a row says of an entry's kind: its type or group or list, and required. */
function kind(item) {
    const what = item.tag === 'value' ? (item.type ?? 'text') : item.tag;
    return item.required ? `${what}, required` : what;
}

/** The markdown of the page. */
export function page(sections) {
    const lines = [
        '# The default invoice map',
        '',
        '`LCrossIndustryInvoice` reads the values of a Factur-X / ZUGFeRD invoice\'s XML through a map, and Lpdf\'s default map applies when a call is given none. This page lists the names it gives, from the map itself, which [you can download](/schema/cross-industry-invoice-map.xml) to start a map of your own. Each comes with the number and the name EN 16931 gives it: BT for a value, BG for a group of them, and BT-X for a value only the EXTENDED profile has.',
        '',
        'A template binds a value under the map\'s prefix, `invoice:`, as `invoice:seller.name`, and repeats an element over a list with `data-source="invoice:lines"`, inside which it binds the item\'s values by their own names, `name` or `net`. Code reads the same names from what `extractInvoiceData` returns: `invoiceData.seller.name`, and `line.net` for each item of `invoiceData.lines`. See [LCrossIndustryInvoice](?p=factur-x-zugferd#from-the-invoice-xml).',
        '',
        'A value the invoice lacks is absent, and a list with no items is empty. A value marked required stops the call with an error that names it when the invoice lacks it, and a required list needs one item. A value of the type `date`, `amount` or `code` goes through the `format` function of its type, with the name in the first column, before it is bound.',
    ];
    for (const { heading, tables } of sections) {
        lines.push('', `## ${heading}`);
        tables.forEach((rows, index) => {
            if (!rows.length) return;
            if (index > 0) lines.push('', 'Less common:');
            lines.push('', '| Name | EN 16931 | Kind |', '|---|---|---|');
            for (const item of rows) lines.push(`| \`${item.fullName}\` | ${item.note} | ${kind(item)} |`);
        });
    }
    return `${lines.join('\n')}\n`;
}

function pagesDir(args) {
    const flag = args.indexOf('--pages');
    if (flag >= 0) return path.resolve(args[flag + 1]);
    return path.resolve(process.env.PAGES_DIR ?? path.join(repo, 'repos/lpdf-pages'));
}

function main() {
    const args = process.argv.slice(2);
    const check = args.includes('--check');
    const pages = pagesDir(args);
    if (!fs.existsSync(path.join(pages, 'www'))) {
        console.error(`The pages checkout is not at ${pages}: give it with --pages <dir> or PAGES_DIR.`);
        process.exit(2);
    }
    const text = fs.readFileSync(MAP, 'utf8');
    const files = [
        [path.join(pages, 'www/schema/cross-industry-invoice-map.xml'), text],
        [path.join(pages, 'www/docs/content/invoice-map.md'), page(readMap(text))],
    ];
    const stale = files.filter(([file, content]) => !fs.existsSync(file) || fs.readFileSync(file, 'utf8') !== content);
    if (check) {
        for (const [file] of stale) console.error(`${path.relative(pages, file)} is not as scripts/sync-invoice-map.mjs writes it`);
        process.exit(stale.length ? 1 : 0);
    }
    for (const [file, content] of stale) {
        fs.mkdirSync(path.dirname(file), { recursive: true });
        fs.writeFileSync(file, content);
        console.log(`Wrote ${path.relative(pages, file)}`);
    }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
