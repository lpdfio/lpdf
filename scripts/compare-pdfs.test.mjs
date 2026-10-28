// Tests of compare-pdfs.mjs: `node --test scripts/compare-pdfs.test.mjs`.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { deflateSync } from 'node:zlib';

import { compare } from './compare-pdfs.mjs';

const SCRIPT = path.join(path.dirname(fileURLToPath(import.meta.url)), 'compare-pdfs.mjs');

/**
 * A small PDF: a page's content, deflated, and a dictionary with `rect`. The offsets of the xref table are made up,
 * and `length` is the value written for /Length, since both move when a number gets longer.
 */
function pdf({ content = '10 20 m 30 40 l S', rect = '10 20 30 40', length = 40, offset = 15, id = 'a'.repeat(32) } = {}) {
    const stream = deflateSync(Buffer.from(content, 'latin1'));
    return Buffer.concat([
        Buffer.from(`%PDF-1.7\n1 0 obj\n<< /Type /Annot /Rect [${rect}] >>\nendobj\n2 0 obj\n<< /Length ${length} /Filter /FlateDecode >>\nstream\n`, 'latin1'),
        stream,
        Buffer.from(`\nendstream\nendobj\nxref\n0 1\n${String(offset).padStart(10, '0')} 00000 n \ntrailer\n<< /ID [<${id}> <${id}>] >>\nstartxref\n${offset + 9}\n%%EOF\n`, 'latin1'),
    ]);
}

test('a PDF compared with itself has nothing to show', () => {
    assert.deepEqual(compare(pdf(), pdf()), { rounding: 0, differences: [] });
});

test('a number that moves by less than the tolerance is rounding, and a longer or shorter one too', () => {
    const result = compare(pdf({ content: '10 603.29004 m 30 40 l S' }), pdf({ content: '10 603.29 m 30 40 l S' }));
    assert.deepEqual(result, { rounding: 1, differences: [] });
});

test('a number that moves by more than the tolerance is a difference, and says where', () => {
    const result = compare(pdf({ content: '10 20 m 30 40 l S' }), pdf({ content: '10 20 m 30 40.01 l S' }));
    assert.equal(result.rounding, 0);
    assert.equal(result.differences.length, 1);
    assert.match(result.differences[0].where, /^stream 1, token 5$/);
    assert.match(result.differences[0].message, /^40 against 40\.01, after: /);
});

test('the tolerance is the one asked for', () => {
    const a = pdf({ content: '10 20 m' });
    const b = pdf({ content: '10 20.01 m' });
    assert.equal(compare(a, b, 0.001).differences.length, 1);
    assert.equal(compare(a, b, 0.1).differences.length, 0);
});

test('a changed word is a difference', () => {
    const result = compare(pdf({ content: '(Hello) Tj' }), pdf({ content: '(Hullo) Tj' }));
    assert.equal(result.differences.length, 1);
});

test('a number of a dictionary is compared, and an annotation that moved is a difference', () => {
    const result = compare(pdf({ rect: '10 20 30 40' }), pdf({ rect: '10 20 30 41' }));
    assert.equal(result.differences.length, 1);
    assert.match(result.differences[0].where, /^objects, token /);
});

test('the offsets, the lengths and the ID, which move with the size of the file, are not compared', () => {
    const result = compare(pdf(), pdf({ length: 42, offset: 17, id: 'b'.repeat(32) }));
    assert.deepEqual(result, { rounding: 0, differences: [] });
});

test('a stream that is not text is compared as bytes', () => {
    const image = (bytes) => Buffer.concat([
        Buffer.from('1 0 obj\n<< /Length 4 >>\nstream\n', 'latin1'), Buffer.from(bytes), Buffer.from('\nendstream\nendobj\n', 'latin1'),
    ]);
    assert.equal(compare(image([0, 1, 2, 3]), image([0, 1, 2, 3])).differences.length, 0);
    assert.deepEqual(compare(image([0, 1, 2, 3]), image([0, 1, 2, 4])).differences, [{ where: 'stream 1', message: 'the bytes differ' }]);
});

test('a PDF with another number of streams is a difference', () => {
    const none = Buffer.from('%PDF-1.7\n1 0 obj\n<< /Type /Catalog >>\nendobj\n', 'latin1');
    assert.equal(compare(pdf(), none).differences.length, 1);
});

/** Runs the script on two folders holding the PDFs given by name. */
function run(before, after, ...options) {
    const root = mkdtempSync(path.join(tmpdir(), 'lpdf-compare-'));
    try {
        const [one, two] = [before, after].map((files, index) => {
            const folder = path.join(root, String(index));
            mkdirSync(folder);
            for (const [name, bytes] of Object.entries(files)) writeFileSync(path.join(folder, name), bytes);
            return folder;
        });
        return spawnSync('node', [SCRIPT, ...options, one, two], { encoding: 'utf8' });
    } finally {
        rmSync(root, { recursive: true, force: true });
    }
}

test('the script exits 0 for PDFs that are the same or differ by rounding, and says so', () => {
    const run0 = run(
        { 'same.pdf': pdf(), 'round.pdf': pdf({ content: '10 603.29004 m' }) },
        { 'same.pdf': pdf(), 'round.pdf': pdf({ content: '10 603.29 m' }) },
    );
    assert.equal(run0.status, 0, run0.stdout + run0.stderr);
    assert.match(run0.stdout, /round\.pdf: rounding only \(1 numbers/);
    assert.match(run0.stdout, /1 unchanged, 1 with rounding only, 0 to look at/);
});

test('the script exits 1 for a PDF that differs by more, and lists the difference', () => {
    const result = run({ 'a.pdf': pdf({ content: '10 20 m' }) }, { 'a.pdf': pdf({ content: '10 25 m' }) });
    assert.equal(result.status, 1, result.stdout + result.stderr);
    assert.match(result.stdout, /a\.pdf: 1 differences above 0\.001/);
    assert.match(result.stdout, /stream 1, token 2: 20 against 25/);
});

test('the script exits 1 for a PDF that is in only one of the folders', () => {
    const result = run({ 'a.pdf': pdf() }, {});
    assert.equal(result.status, 1);
    assert.match(result.stdout, /a\.pdf: in only one of the folders/);
});

test('the script exits 2 without two paths', () => {
    const result = spawnSync('node', [SCRIPT, 'one'], { encoding: 'utf8' });
    assert.equal(result.status, 2);
    assert.match(result.stderr, /usage: /);
});
