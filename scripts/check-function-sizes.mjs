// Fails when a function of the engine's WASI build is too large for the JVM. The Java SDK runs the engine on Endive,
// which compiles each WASM function to one JVM method when the SDK is built, and a JVM method holds at most 64 KB of
// bytecode: a function over it doesn't compile, and the SDK can't be built.
//
// Endive's bytecode came to 1.1 to 1.65 times a function's size in the WASM, 1.54 at most for a function of 10 KB or
// more (measured on 2026-11-04, Endive 1.2.0). A function of up to 40 KB stays under the JVM's limit at a ratio of 1.6,
// so that is the limit here. A function over it is split, or a helper it inlines is kept out of it with
// `#[inline(never)]`, as `assemble_pdf`'s are in `pdf.rs`. The check reads the WASM alone: core builds no Java for it,
// and depends on no SDK.
//
// Usage: node scripts/check-function-sizes.mjs [wasm]    the WASI build, dist/wasi/lpdf.wasm by default; lists the
//        functions over the limit and exits 1 if there are any

import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

/** The most bytes a function's code may take in the WASM. */
export const LIMIT = 40 * 1024;

/**
 * The functions of a WASM module: the index of each, the size of its code in bytes, and its name from the name
 * section, which a Rust build keeps.
 */
export function functionSizes(bytes) {
    let at = 8;
    const u32 = () => {
        let value = 0;
        for (let shift = 0; ; shift += 7) {
            const byte = bytes[at++];
            value += (byte & 0x7f) * 2 ** shift;
            if (!(byte & 0x80)) return value;
        }
    };
    const limits = () => {
        const flags = bytes[at++];
        u32();
        if (flags & 1) u32();
    };
    // Not `at += u32()`, which adds the length to where `at` was before the length was read.
    const skipName = () => {
        const length = u32();
        at += length;
    };

    let imported = 0;
    const functions = [];
    const names = new Map();
    while (at < bytes.length) {
        const id = bytes[at++];
        const size = u32();
        const end = at + size;
        if (id === 2) {
            // The imported functions come first in the index space, before the module's own.
            for (let count = u32(); count > 0; count--) {
                skipName();
                skipName();
                const kind = bytes[at++];
                if (kind === 0) { u32(); imported++; }
                else if (kind === 1) { at++; limits(); }
                else if (kind === 2) limits();
                else if (kind === 3) at += 2;
                else if (kind === 4) { at++; u32(); }
            }
        } else if (id === 10) {
            for (let count = u32(), i = 0; i < count; i++) {
                const length = u32();
                functions.push({ index: imported + i, size: length });
                at += length;
            }
        } else if (id === 0) {
            const nameLength = u32();
            const name = bytes.subarray(at, at + nameLength).toString('utf8');
            at += nameLength;
            while (name === 'name' && at < end) {
                const subsection = bytes[at++];
                const subsectionSize = u32();
                const subsectionEnd = at + subsectionSize;
                if (subsection === 1) {
                    for (let count = u32(); count > 0; count--) {
                        const index = u32();
                        const length = u32();
                        names.set(index, bytes.subarray(at, at + length).toString('utf8'));
                        at += length;
                    }
                }
                at = subsectionEnd;
            }
        }
        at = end;
    }
    return functions.map((f) => ({ ...f, name: names.get(f.index) ?? `function ${f.index}` }));
}

/**
 * A Rust symbol as a path of its names, `lpdf_wasi::pdf::assemble_pdf` for a v0 symbol such as
 * `_RNvNtCs…_9lpdf_wasi3pdf12assemble_pdf`: each name is its length and its letters, after the crate's hash. Any other
 * symbol is left as it is.
 */
export function readable(symbol) {
    if (!symbol.startsWith('_R')) return symbol;
    const text = symbol.replace(/Cs[0-9A-Za-z]*_/g, ' ');
    const parts = [];
    for (let at = 0; at < text.length;) {
        const digits = /^\d+/.exec(text.slice(at));
        if (digits && /[A-Za-z_]/.test(text[at + digits[0].length] ?? '')) {
            const start = at + digits[0].length;
            parts.push(text.slice(start, start + Number(digits[0])));
            at = start + Number(digits[0]);
        } else {
            at++;
        }
    }
    return parts.length ? parts.join('::') : symbol;
}

function main() {
    const file = process.argv[2] ?? 'dist/wasi/lpdf.wasm';
    const functions = functionSizes(readFileSync(file)).sort((a, b) => b.size - a.size);
    const over = functions.filter((f) => f.size > LIMIT);
    for (const f of over) {
        console.log(`${readable(f.name)} is ${f.size} bytes, over the limit of ${LIMIT}: split it, or keep a helper it `
            + 'inlines out of it with #[inline(never)]');
    }
    const largest = functions[0];
    console.log(over.length
        ? `${over.length} of ${functions.length} functions of ${file} are over ${LIMIT} bytes`
        : `all ${functions.length} functions of ${file} are within ${LIMIT} bytes; the largest is `
            + `${readable(largest.name)}, ${largest.size} bytes`);
    if (over.length) process.exit(1);
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) main();
