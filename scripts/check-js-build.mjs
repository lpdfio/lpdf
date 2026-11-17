// Runs the JS build of the engine, as wasm-pack builds it for the release, through the calls the SDKs make, and checks
// that each gives a PDF. The core's tests run natively, where the system allocator is C's, so they can't see a fault
// that only the JS build's code has, such as an optimization that turns the build's own allocator against it.
//
//  - A render, a render encrypted with AES-256 and one with RC4-128, each with a user and an owner password: the file
//    is a PDF, and an encrypted one has the cipher's /V in its /Encrypt dictionary.
//  - A merge of two files, and a merge of a file whose owner restricts printing, refused without allow-restricted and
//    merged with it, which reads AES-256 in the JS build.
//
// Each check runs on an engine of its own, loaded afresh, so a trap in one doesn't fail the next.
//
// Usage: node scripts/check-js-build.mjs <folder>   the folder of `wasm-pack build src/core --target nodejs`; lists the
//        failures, exit 1 if any

import { randomBytes } from 'node:crypto';
import { createRequire } from 'node:module';
import path from 'node:path';

const folder = process.argv[2];
if (!folder) {
    console.error('usage: node scripts/check-js-build.mjs <folder of the nodejs build>');
    process.exit(2);
}
const glue = path.resolve(folder, 'lpdf.js');
const require = createRequire(import.meta.url);

/** A new engine from a fresh instance of the build. */
function engine() {
    delete require.cache[glue];
    const { LpdfEngine } = require(glue);
    return new LpdfEngine('');
}

/** A one-page document, with `output` in it. */
function xml(output = '') {
    return `<lpdf version="1"><document>${output}<section><layout><text>Hello</text></layout></section></document></lpdf>`;
}

function render(output, user, owner) {
    return engine().render_pdf(xml(output), undefined, user, owner, false, new Uint8Array(randomBytes(32)));
}

function latin1(bytes) {
    return Buffer.from(bytes).toString('latin1');
}

function pages(bytes) {
    return (latin1(bytes).match(/\/Type\s*\/Page\b/g) ?? []).length;
}

function merge(files, options = '') {
    const pdfs = Buffer.concat(files.map((f) => Buffer.from(f.pdf)));
    const index = JSON.stringify(files.map((f) => ({ name: f.name, length: f.pdf.length })));
    return engine().merge(new Uint8Array(pdfs), index, options);
}

function expect(condition, what) {
    if (!condition) throw new Error(what);
}

const checks = {
    'a render gives a PDF'() {
        const pdf = render('', undefined, undefined);
        expect(latin1(pdf.subarray(0, 5)) === '%PDF-', 'the output does not start with %PDF-');
    },
    'an AES-256 render with passwords encrypts the file'() {
        const pdf = render('<output encrypt="aes-256" allow="print"/>', 'user', 'owner');
        expect(/\/Encrypt\s+\d+ 0 R/.test(latin1(pdf)), 'the file has no /Encrypt');
        expect(/\/V\s+5\b/.test(latin1(pdf)), 'the /Encrypt dictionary is not /V 5');
    },
    'an RC4-128 render with passwords encrypts the file'() {
        const pdf = render('<output encrypt="rc4-128" allow="print"/>', 'user', 'owner');
        expect(/\/V\s+2\b/.test(latin1(pdf)), 'the /Encrypt dictionary is not /V 2');
    },
    'a merge of two files gives their pages'() {
        const one = render('', undefined, undefined);
        const merged = merge([{ name: 'a.pdf', pdf: one }, { name: 'b.pdf', pdf: one }]);
        expect(pages(merged) === 2, `the merged file has ${pages(merged)} pages, not 2`);
    },
    'a merge refuses a restricted file, and merges it with allow-restricted'() {
        const limited = render('<output encrypt="aes-256" allow="print"/>', undefined, 'owner');
        let refusal = null;
        try {
            merge([{ name: 'limited.pdf', pdf: limited }]);
        } catch (e) {
            refusal = e;
        }
        expect(typeof refusal === 'string' && refusal.startsWith('limited.pdf: the owner restricts'),
            `the merge without allow-restricted gave ${refusal instanceof Error ? refusal.stack : refusal}`);
        const merged = merge([{ name: 'limited.pdf', pdf: limited }], '{"allow-restricted": true}');
        expect(pages(merged) === 1 && !latin1(merged).includes('/Encrypt'), 'the merged file is not one unencrypted page');
    },
};

let failures = 0;
for (const [name, check] of Object.entries(checks)) {
    try {
        check();
        console.log(`ok    ${name}`);
    } catch (e) {
        failures++;
        const why = e instanceof WebAssembly.RuntimeError
            ? `the engine trapped: ${e.stack.split('\n').slice(0, 16).join('\n')}`
            : (e instanceof Error ? e.message : String(e));
        console.log(`FAIL  ${name}: ${why}`);
    }
}
if (failures) {
    console.log(`\n${failures} of ${Object.keys(checks).length} checks failed`);
    process.exit(1);
}
