// Tests of check-function-sizes.mjs: node --test scripts/check-function-sizes.test.mjs

import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { test } from 'node:test';

import { functionSizes, LIMIT, readable } from './check-function-sizes.mjs';

const SCRIPT = path.join(import.meta.dirname, 'check-function-sizes.mjs');

function u32(value) {
    const bytes = [];
    do {
        let byte = value & 0x7f;
        value = Math.floor(value / 128);
        if (value) byte |= 0x80;
        bytes.push(byte);
    } while (value);
    return bytes;
}

const section = (id, content) => [id, ...u32(content.length), ...content];
const text = (s) => [...u32(Buffer.byteLength(s)), ...Buffer.from(s)];

/**
 * A module that imports one function and has a function of each size in `sizes`, each code of that many bytes
 * (no locals, nops, end), named in its name section.
 */
function module(sizes, names) {
    const types = section(1, [1, 0x60, 0, 0]);
    const imports = section(2, [1, ...text('env'), ...text('f'), 0, 0]);
    const functions = section(3, [...u32(sizes.length), ...sizes.flatMap(() => [0])]);
    const bodies = sizes.flatMap((size) => [...u32(size), 0, ...new Array(size - 2).fill(0x01), 0x0b]);
    const code = section(10, [...u32(sizes.length), ...bodies]);
    const entries = names.flatMap((name, i) => [...u32(i + 1), ...text(name)]);
    const nameMap = [...u32(names.length), ...entries];
    const custom = section(0, [...text('name'), 1, ...u32(nameMap.length), ...nameMap]);
    return Buffer.from([0x00, 0x61, 0x73, 0x6d, 1, 0, 0, 0, ...types, ...imports, ...functions, ...code, ...custom]);
}

test('reads the size and name of each function after the imported ones', () => {
    const functions = functionSizes(module([10, 300], ['small', 'large']));

    assert.deepEqual(functions, [
        { index: 1, size: 10, name: 'small' },
        { index: 2, size: 300, name: 'large' },
    ]);
});

test('names a Rust v0 symbol by the path of its names', () => {
    assert.equal(readable('_RNvNtCsh8FL6neZVcw_9lpdf_wasi3pdf12assemble_pdf'), 'lpdf_wasi::pdf::assemble_pdf');
    assert.equal(readable('memcpy'), 'memcpy');
});

function check(sizes) {
    const folder = mkdtempSync(path.join(tmpdir(), 'function-sizes-'));
    const file = path.join(folder, 'module.wasm');
    writeFileSync(file, module(sizes, sizes.map((_, i) => `f${i}`)));
    try {
        return { status: 0, output: execFileSync('node', [SCRIPT, file], { encoding: 'utf8' }) };
    } catch (error) {
        return { status: error.status, output: error.stdout };
    } finally {
        rmSync(folder, { recursive: true });
    }
}

test('passes a module whose largest function is at the limit', () => {
    const { status, output } = check([100, LIMIT]);

    assert.equal(status, 0);
    assert.match(output, new RegExp(`the largest is f1, ${LIMIT} bytes`));
});

test('fails a module with a function over the limit, naming it', () => {
    const { status, output } = check([100, LIMIT + 1]);

    assert.equal(status, 1);
    assert.match(output, new RegExp(`f1 is ${LIMIT + 1} bytes, over the limit of ${LIMIT}`));
});
