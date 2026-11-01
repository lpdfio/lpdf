// Opens the linearized PDFs the core writes with qpdf, an implementation of the PDF standard that shares no code with
// ours, and checks what a viewer that fetches the file by ranges relies on:
//
//  - qpdf finds the file linearized, and `--check-linearization` reads the hint tables back against the file with no
//    error: the first page's section, its end, the page offsets and lengths, the shared objects, the outline;
//  - `--check` finds no error in the file's structure or its streams. A warning qpdf gives the same document rendered
//    without linearize, which linearizing does not bring, is not counted against the linearized one;
//  - an encrypted file reports the cipher it was asked for, opens with the user password and with the owner password,
//    refuses a wrong one, and is linearized under each;
//  - each page shows what the same document shows rendered without linearize: its dictionary, its content and every
//    object they reach, compared with references followed, as the objects have other numbers. A linearized page names
//    only the resources its content draws, so it may name fewer than the other, but none that differs, and none its
//    content uses is missing. A transparency group, a form the page draws, is compared the same way.
//
// qpdf before 12 sorts one kind of object otherwise: one that a page other than page 1 reaches, and a key of the
// catalog too, it puts with the rest of the file rather than with the page. In a tagged file the structure reaches the
// link annotations, so for each page after page 1 with a link it counts fewer objects than the hints, and says
// "object count mismatch". The engine sorts them as qpdf 12 does, as Annex F has it (`linearize.rs`), and with an
// older qpdf that warning alone is let through for the tagged sample. Ubuntu 24.04, where core's CI runs, has 11.9.
//
// The samples come from the core's own tests:
//   LPDF_LINEARIZE_OUT=<folder> cargo test --manifest-path src/core/Cargo.toml --lib writes_linearized_samples_for_the_external_check
//
// Usage: node scripts/check-linearization.mjs <folder>     list the failures, exit 1 if any
//        needs qpdf on the PATH (`apt install qpdf`, `brew install qpdf`)

import { spawnSync } from 'node:child_process';
import path from 'node:path';

const folder = process.argv[2];
if (!folder) {
    console.error('usage: node scripts/check-linearization.mjs <folder of samples>');
    process.exit(2);
}

/**
 * The samples `writes_linearized_samples_for_the_external_check` writes, each with the render it is compared with,
 * and for an encrypted one its passwords and the revision of its cipher.
 */
const SAMPLES = [
    { file: 'one-page.pdf', plain: 'one-page-plain.pdf' },
    { file: 'pages.pdf', plain: 'pages-plain.pdf' },
    { file: 'encrypted-aes256.pdf', plain: 'encrypted-plain.pdf', user: 'user', owner: 'owner', revision: 6 },
    { file: 'encrypted-rc4.pdf', plain: 'encrypted-plain.pdf', user: 'user', owner: 'owner', revision: 3 },
    { file: 'forms.pdf', plain: 'forms-plain.pdf' },
    { file: 'pdfua.pdf', plain: 'pdfua-plain.pdf', tagged: true },
    { file: 'book.pdf', plain: 'book-plain.pdf' },
];

const failures = [];

/** The major version of the qpdf on the PATH. */
const QPDF_MAJOR = Number(/qpdf version (\d+)/.exec(qpdf('--version').stdout)?.[1] ?? 0);

/** The warnings qpdf gave, without the file, object and offset they name, which differ from file to file. */
function warnings(output) {
    return output.split('\n')
        .filter((line) => line.startsWith('WARNING: '))
        .map((line) => line.replace(/^WARNING: [^:]*?(?: \(object \d+ \d+, offset \d+\))?: /, ''));
}

/** Whether a warning about the linearized file is let through: the plain render has it too, or it is the one an older
 * qpdf gives a tagged file. */
function known(warning, plainWarnings, sample) {
    if (plainWarnings.has(warning) || warning === 'operation succeeded with warnings') return true;
    return Boolean(sample.tagged) && QPDF_MAJOR < 12 && /^object count mismatch for page \d+: /.test(warning);
}

/** Runs qpdf; a non-zero exit is data here, not an error. Its version is read before the samples. */
function qpdf(...args) {
    const run = spawnSync('qpdf', args, { maxBuffer: 256 * 1024 * 1024 });
    if (run.error) {
        console.error(`could not run qpdf: ${run.error.message}`);
        process.exit(2);
    }
    return { status: run.status, stdout: run.stdout.toString('latin1'), output: `${run.stdout}${run.stderr}` };
}

function expect(ok, message) {
    if (!ok) failures.push(message);
}

/** The file's objects, by `N G R`, and its pages, from qpdf's JSON, with the streams decoded and decrypted. */
function load(file, password) {
    const run = qpdf('--json=2', '--json-stream-data=inline', '--decode-level=generalized', '--decrypt',
        `--password=${password ?? ''}`, file);
    if (run.status !== 0 && run.status !== 3) {
        return { error: run.output.trim() };
    }
    const json = JSON.parse(run.stdout);
    const objects = new Map();
    for (const [key, value] of Object.entries(json.qpdf[1])) {
        const match = /^obj:(\d+ \d+ R)$/.exec(key);
        if (match) objects.set(match[1], value);
    }
    return { objects, pages: json.pages.map((page) => page.object) };
}

/** Keys that hold where an object is rather than what it shows: back to the page tree or the structure, or a length. */
const PLACE_KEYS = new Set(['/Parent', '/P', '/Pg', '/StructParent', '/StructParents', '/Length']);

/** `value` with every reference replaced by what it refers to, once on each path. */
function resolve(objects, value, seen = new Set()) {
    if (typeof value === 'string' && /^\d+ \d+ R$/.test(value)) {
        if (seen.has(value)) return 'loop';
        const object = objects.get(value);
        if (!object) return null;
        const next = new Set(seen).add(value);
        if (object.stream) {
            return { stream: resolve(objects, object.stream.dict, next), data: object.stream.data ?? null };
        }
        return resolve(objects, object.value, next);
    }
    if (Array.isArray(value)) return value.map((item) => resolve(objects, item, seen));
    if (value && typeof value === 'object') {
        return Object.fromEntries(Object.entries(value)
            .filter(([key]) => !PLACE_KEYS.has(key))
            .map(([key, item]) => [key, resolve(objects, item, seen)]));
    }
    return value;
}

const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

/** The names a page's content uses. */
function namesUsed(content) {
    const data = content?.data ? Buffer.from(content.data, 'base64').toString('latin1') : '';
    return new Set([...data.matchAll(/\/([^\s/[\]<>(){}%]+)/g)].map((match) => `/${match[1]}`));
}

/**
 * Compares what a page or a form draws, `a` from the plain render and `b` from the linearized one: every key but where
 * it sits and its resources, and its resources by name. `b` may name fewer, as it names only what its content draws,
 * but each it names is the same, a form compared as a page is, and none its content draws is missing.
 */
function compareDrawn(where, plain, a, linearized, b, depth = 0) {
    if (depth > 8) return;
    for (const key of new Set([...Object.keys(a), ...Object.keys(b)])) {
        if (key === '/Parent' || key === '/Resources') continue;
        expect(same(resolve(plain.objects, a[key]), resolve(linearized.objects, b[key])), `${where}: ${key} differs`);
    }
    const used = namesUsed(b.content);
    const plainResources = deref(plain, a['/Resources']) ?? {};
    const resources = deref(linearized, b['/Resources']) ?? {};
    for (const kind of new Set([...Object.keys(plainResources), ...Object.keys(resources)])) {
        const plainKind = deref(plain, plainResources[kind]) ?? {};
        const kindOf = deref(linearized, resources[kind]) ?? {};
        if (typeof plainKind !== 'object' || Array.isArray(plainKind)) {
            expect(same(resolve(plain.objects, plainKind), resolve(linearized.objects, kindOf)), `${where}: resources ${kind} differ`);
            continue;
        }
        for (const [name, ref] of Object.entries(kindOf)) {
            const plainForm = drawn(plain, plainKind[name]);
            const form = drawn(linearized, ref);
            if (plainForm && form) {
                compareDrawn(`${where} ${kind}${name}`, plain, plainForm, linearized, form, depth + 1);
            } else {
                expect(same(resolve(plain.objects, plainKind[name]), resolve(linearized.objects, ref)), `${where}: resource ${kind}${name} differs`);
            }
        }
        for (const name of Object.keys(plainKind)) {
            expect(!used.has(name) || name in kindOf, `${where}: resource ${kind}${name} is drawn but not named`);
        }
    }
}

/** The value `value` refers to, one step. */
function deref(file, value) {
    if (typeof value === 'string' && /^\d+ \d+ R$/.test(value)) {
        const object = file.objects.get(value);
        return object?.stream ? object.stream.dict : object?.value;
    }
    return value;
}

/** A form XObject as a dictionary of its keys with its decoded content under `content`, or null for anything else. */
function drawn(file, ref) {
    const object = typeof ref === 'string' ? file.objects.get(ref) : null;
    if (!object?.stream || object.stream.dict['/Subtype'] !== '/Form') return null;
    return { ...object.stream.dict, content: { data: object.stream.data } };
}

function comparePages(name, plain, linearized) {
    expect(plain.pages.length === linearized.pages.length,
        `${name}: ${linearized.pages.length} pages against ${plain.pages.length}`);
    plain.pages.forEach((plainPage, index) => {
        const a = plain.objects.get(plainPage).value;
        const b = linearized.objects.get(linearized.pages[index]).value;
        compareDrawn(`${name} page ${index + 1}`,
            plain, { ...a, content: resolve(plain.objects, a['/Contents']) },
            linearized, { ...b, content: resolve(linearized.objects, b['/Contents']) });
    });
}

for (const sample of SAMPLES) {
    const file = path.join(folder, sample.file);
    const name = sample.file;
    const passwords = sample.owner ? [['user', sample.user], ['owner', sample.owner]] : [['none', '']];

    const plainWarnings = new Set(warnings(qpdf('--check', path.join(folder, sample.plain)).output));
    for (const [who, password] of passwords) {
        const lin = qpdf('--check-linearization', `--password=${password}`, file);
        const linUnknown = warnings(lin.output).filter((warning) => !known(warning, plainWarnings, sample));
        expect(lin.status !== 2 && linUnknown.length === 0
            && (lin.output.includes('no linearization errors') || warnings(lin.output).length > 0),
            `${name}: with the ${who} password, qpdf's linearization check fails: ${lin.output.trim()}`);
        const check = qpdf('--check', `--password=${password}`, file);
        const checkUnknown = warnings(check.output).filter((warning) => !known(warning, plainWarnings, sample));
        expect(check.status !== 2 && checkUnknown.length === 0 && check.output.includes('File is linearized'),
            `${name}: with the ${who} password, qpdf --check fails: ${check.output.trim()}`);
    }

    if (sample.owner) {
        const shown = qpdf('--show-encryption', `--password=${sample.owner}`, file);
        expect(shown.output.includes(`R = ${sample.revision}`), `${name}: expected R = ${sample.revision}`);
        const wrong = qpdf('--check', '--password=not-the-password', file);
        expect(wrong.status === 2 && /invalid password/.test(wrong.output), `${name}: a wrong password was not refused`);
    }

    const plain = load(path.join(folder, sample.plain));
    const linearized = load(file, sample.user);
    if (plain.error || linearized.error) {
        failures.push(`${name}: qpdf could not read it as JSON: ${plain.error ?? linearized.error}`);
        continue;
    }
    comparePages(name, plain, linearized);
}

if (failures.length > 0) {
    console.error(failures.map((failure) => `FAIL ${failure}`).join('\n'));
    process.exit(1);
}
console.log(`linearization check passed with qpdf ${QPDF_MAJOR}: ${SAMPLES.length} files, each linearized by qpdf's reading and showing the pages of its render without linearize`);
