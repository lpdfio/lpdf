// Compares two PDFs, or two folders of them, and says whether they differ by more than rounding.
//
// A change that moves a coordinate in its last digit (603.29004 for 603.29) changes the hash of a snapshot without
// changing a page, and when it does that to dozens of them, a real change hides among them. This reads each PDF as
// its objects and its inflated streams and compares them token by token: a number that differs by no more than the
// tolerance, in points, is rounding, and anything else is a difference to look at, as in the `lpdf-pdf-diff-review`
// skill.
//
//  - The xref offsets, the values of /Length and startxref, and the hex strings (the /ID, and what an encryption
//    writes) are left out: they move whenever a number gets shorter or longer.
//  - A stream that isn't text once inflated, such as an image, or that can't be inflated, such as an encrypted one,
//    is compared as bytes. Every stream of an encrypted PDF differs for any change at all.
//
// Usage: node scripts/compare-pdfs.mjs [--tolerance <points>] <old> <new>
//        <old> and <new> are two PDFs, or two folders: the PDFs of <old> are compared with those of <new> by name.
//        Lists the PDFs that differ, and exits 1 if any of them differs by more than the tolerance.

import { readdirSync, readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { inflateSync } from 'node:zlib';

/** Points. A coordinate is an f32 that the writer prints in full, so one step of its last digit is up to about 1e-4. */
export const DEFAULT_TOLERANCE = 0.001;

/** How many differences of a PDF are listed. */
const SHOWN = 5;

/** A number, a run of anything else that isn't a space, or a minus sign that isn't part of a number. */
const TOKEN = /-?\d+(?:\.\d+)?|[^\d\s-]+|-/g;
const NUMBER = /^-?\d+(?:\.\d+)?$/;

/** An xref offset, and a long hex string: values that move with the length of the file, and say nothing about its pages. */
const OFFSET = /^\d{10}$/;
const HEX_STRING = /<[0-9a-fA-F]{16,}>/g;

/** True when a buffer holds text, as the content of a page does, and not the bytes of an image or a font. */
function isText(buffer) {
    return !buffer.subarray(0, 512).some((byte) => byte < 9 || (byte > 13 && byte < 32));
}

/**
 * A PDF as the parts to compare: its text outside the streams, and each stream, inflated when it can be. A part is
 * `{ label, text }` for text, or `{ label, bytes }` for a stream to compare as bytes.
 */
export function parts(file) {
    const text = file.toString('latin1');
    const found = [];
    let outside = '';
    let from = 0;
    for (const start of text.matchAll(/(?<!end)stream\r?\n/g)) {
        if (start.index < from) continue;
        const bodyStart = start.index + start[0].length;
        const bodyEnd = text.indexOf('endstream', bodyStart);
        if (bodyEnd < 0) break;
        outside += text.slice(from, bodyStart);
        from = bodyEnd;
        const body = file.subarray(bodyStart, bodyEnd);
        let content;
        try {
            content = inflateSync(body);
        } catch {
            content = null;
        }
        const label = `stream ${found.length + 1}`;
        found.push(content && isText(content) ? { label, text: content.toString('latin1') } : { label, bytes: content ?? body });
    }
    outside += text.slice(from);
    return [{ label: 'objects', text: outside }, ...found];
}

/** The tokens of a text that say something about the pages. */
function tokens(text) {
    const all = text.replace(HEX_STRING, ' ').match(TOKEN) ?? [];
    return all.filter((token, index) => !OFFSET.test(token) && all[index - 1] !== '/Length' && all[index - 1] !== 'startxref');
}

/**
 * What differs between two PDFs: `rounding` counts the numbers that differ by no more than `tolerance`, and
 * `differences` lists the rest, each as `{ where, message }`.
 */
export function compare(before, after, tolerance = DEFAULT_TOLERANCE) {
    const result = { rounding: 0, differences: [] };
    const one = parts(before);
    const other = parts(after);
    if (one.length !== other.length) {
        result.differences.push({ where: 'streams', message: `${one.length - 1} streams against ${other.length - 1}` });
        return result;
    }
    one.forEach((part, index) => {
        const next = other[index];
        if (part.bytes || next.bytes) {
            const same = part.bytes && next.bytes && Buffer.compare(part.bytes, next.bytes) === 0;
            if (!same) result.differences.push({ where: part.label, message: 'the bytes differ' });
            return;
        }
        const a = tokens(part.text);
        const b = tokens(next.text);
        if (a.length !== b.length) {
            result.differences.push({ where: part.label, message: `${a.length} tokens against ${b.length}` });
            return;
        }
        a.forEach((token, at) => {
            if (token === b[at]) return;
            if (NUMBER.test(token) && NUMBER.test(b[at]) && Math.abs(Number(token) - Number(b[at])) <= tolerance) {
                result.rounding++;
                return;
            }
            const around = a.slice(Math.max(0, at - 5), at + 1).join(' ');
            result.differences.push({ where: `${part.label}, token ${at + 1}`, message: `${token} against ${b[at]}, after: ${around}` });
        });
    });
    return result;
}

/** The PDFs to compare, as `[name, old file, new file]`: one pair for two files, or those with the same name in two folders. */
function pairs(before, after) {
    if (statSync(before).isFile() && statSync(after).isFile()) return [[path.basename(after), before, after]];
    const names = (folder) => readdirSync(folder).filter((name) => name.endsWith('.pdf')).sort();
    const left = names(before);
    const right = names(after);
    const both = left.filter((name) => right.includes(name));
    const missing = [...left.filter((name) => !right.includes(name)), ...right.filter((name) => !left.includes(name))];
    return [...both.map((name) => [name, path.join(before, name), path.join(after, name)]), ...missing.map((name) => [name, null, null])];
}

function main(args) {
    let tolerance = DEFAULT_TOLERANCE;
    const folders = [];
    for (let i = 0; i < args.length; i++) {
        if (args[i] === '--tolerance') tolerance = Number(args[++i]);
        else folders.push(args[i]);
    }
    if (folders.length !== 2 || !(tolerance >= 0)) {
        console.error('usage: node scripts/compare-pdfs.mjs [--tolerance <points>] <old pdf or folder> <new pdf or folder>');
        process.exit(2);
    }

    let unchanged = 0;
    let roundingOnly = 0;
    let above = 0;
    for (const [name, left, right] of pairs(...folders)) {
        if (!left) {
            console.log(`${name}: in only one of the folders`);
            above++;
            continue;
        }
        const before = readFileSync(left);
        const after = readFileSync(right);
        if (before.equals(after)) {
            unchanged++;
            continue;
        }
        const { rounding, differences } = compare(before, after, tolerance);
        if (differences.length === 0) {
            console.log(`${name}: rounding only (${rounding} numbers differ by no more than ${tolerance})`);
            roundingOnly++;
            continue;
        }
        above++;
        console.log(`${name}: ${differences.length} differences above ${tolerance}, and ${rounding} numbers of rounding`);
        for (const { where, message } of differences.slice(0, SHOWN)) console.log(`    ${where}: ${message}`);
        if (differences.length > SHOWN) console.log(`    and ${differences.length - SHOWN} more`);
    }
    console.log(`${unchanged} unchanged, ${roundingOnly} with rounding only, ${above} to look at`);
    process.exit(above > 0 ? 1 : 0);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    main(process.argv.slice(2));
}
