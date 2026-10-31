// Opens the encrypted PDFs the core writes with qpdf, an implementation of the PDF standard that
// shares no code with ours, and checks what a reader would see:
//
//  - the file reports the algorithm it was asked for (R = 6 and AESv3, or R = 3 for RC4);
//  - it opens with the user password and with the owner password, and a wrong password, or none for a
//    file that has a user password, is refused;
//  - decrypting it gives the same objects as the document rendered without encryption: the QDF form
//    of the two is identical apart from the /ID line, so every string and stream survived the round trip;
//  - for a document with files in it, each file it gives back with the password is the file of the
//    unencrypted render, so the files are encrypted with the rest and come back whole.
//
// The samples come from the core's own tests:
//   LPDF_ENCRYPT_OUT=<folder> cargo test --manifest-path src/core/Cargo.toml --lib writes_samples_for_the_external_check
//
// Usage: node scripts/check-encryption.mjs <folder>     list the failures, exit 1 if any
//        needs qpdf on the PATH (`apt install qpdf`, `brew install qpdf`)

import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';

const folder = process.argv[2];
if (!folder) {
    console.error('usage: node scripts/check-encryption.mjs <folder of samples>');
    process.exit(2);
}

/**
 * The samples `writes_samples_for_the_external_check` writes: who may open each, what it must report, the
 * unencrypted render it must decrypt to, and the files it carries.
 */
const SAMPLES = [
    { file: 'aes256-user.pdf', user: 'user', owner: 'owner', revision: 6, method: 'AESv3', plain: 'plain.pdf' },
    { file: 'aes256-open.pdf', user: '', owner: 'owner', revision: 6, method: 'AESv3', plain: 'plain.pdf' },
    { file: 'aes256-same-password.pdf', user: 'same', owner: 'same', revision: 6, method: 'AESv3', plain: 'plain.pdf' },
    { file: 'rc4-user.pdf', user: 'user', owner: 'owner', revision: 3, method: null, plain: 'plain.pdf' },
    {
        file: 'attachments-aes256.pdf', user: 'user', owner: 'owner', revision: 6, method: 'AESv3',
        plain: 'attachments-plain.pdf', attachments: ['invoice.xml', 'terms.txt'],
    },
];

const scratch = mkdtempSync(path.join(tmpdir(), 'lpdf-encryption-'));
const failures = [];

/** Runs qpdf; a non-zero exit is data here, not an error. `stdout` keeps the bytes, as a file comes back. */
function qpdf(...args) {
    const run = spawnSync('qpdf', args, { maxBuffer: 64 * 1024 * 1024 });
    if (run.error) {
        console.error(`could not run qpdf: ${run.error.message}`);
        process.exit(2);
    }
    return { status: run.status, stdout: run.stdout, output: `${run.stdout.toString('latin1')}${run.stderr.toString('latin1')}` };
}

function expect(ok, message) {
    if (!ok) failures.push(message);
}

/** The QDF form of a file without its /ID, which qpdf makes up for a file that has none. */
function qdf(file, password) {
    const out = path.join(scratch, `${path.basename(file)}.${password || 'none'}.qdf`);
    const run = qpdf('--qdf', '--object-streams=disable', '--decrypt', `--password=${password}`, file, out);
    if (run.status !== 0) return { error: run.output.trim() };
    const text = readFileSync(out, 'latin1');
    return { lines: text.split('\n').filter((line) => !/^\s*\/ID \[/.test(line)) };
}

/** The QDF form of each unencrypted render the samples are compared with, by file name. */
const plains = new Map();
for (const name of new Set(SAMPLES.map((sample) => sample.plain))) {
    const plain = qdf(path.join(folder, name), '');
    if (plain.error) {
        console.error(`${name}: ${plain.error}`);
        process.exit(1);
    }
    plains.set(name, plain);
}

for (const sample of SAMPLES) {
    const file = path.join(folder, sample.file);
    const name = sample.file;
    const plain = plains.get(sample.plain);

    const shown = qpdf('--show-encryption', `--password=${sample.owner}`, file);
    expect(shown.status === 0, `${name}: qpdf could not read the encryption: ${shown.output.trim()}`);
    expect(shown.output.includes(`R = ${sample.revision}`), `${name}: expected R = ${sample.revision}`);
    if (sample.method) {
        for (const kind of ['stream', 'string', 'file']) {
            expect(
                shown.output.includes(`${kind} encryption method: ${sample.method}`),
                `${name}: expected ${kind} encryption method ${sample.method}`,
            );
        }
    }

    for (const [who, password] of [['user', sample.user], ['owner', sample.owner]]) {
        const checked = qpdf('--check', `--password=${password}`, file);
        expect(checked.status === 0, `${name}: does not open with the ${who} password: ${checked.output.trim()}`);

        const decrypted = qdf(file, password);
        if (decrypted.error) {
            failures.push(`${name}: cannot decrypt with the ${who} password: ${decrypted.error}`);
            continue;
        }
        const at = decrypted.lines.findIndex((line, i) => line !== plain.lines[i]);
        expect(
            at === -1 && decrypted.lines.length === plain.lines.length,
            `${name}: decrypted with the ${who} password it differs from the unencrypted render at line ${at + 1}: `
                + `${JSON.stringify(decrypted.lines[at])} against ${JSON.stringify(plain.lines[at])}`,
        );
    }

    for (const attachment of sample.attachments ?? []) {
        const shown = qpdf(`--show-attachment=${attachment}`, `--password=${sample.user}`, file);
        const expected = qpdf(`--show-attachment=${attachment}`, path.join(folder, sample.plain));
        expect(
            shown.status === 0 && expected.status === 0 && Buffer.compare(shown.stdout, expected.stdout) === 0,
            `${name}: the file ${attachment} does not come back whole with the user password: ${shown.output.trim()}`,
        );
    }

    const wrong = qpdf('--check', '--password=not-the-password', file);
    expect(wrong.status === 2 && /invalid password/.test(wrong.output), `${name}: a wrong password was not refused`);

    const none = qpdf('--check', file);
    if (sample.user === '') {
        expect(none.status === 0, `${name}: has no user password but does not open without one: ${none.output.trim()}`);
    } else {
        expect(none.status === 2 && /invalid password/.test(none.output), `${name}: opened without the user password`);
    }
}

rmSync(scratch, { recursive: true, force: true });

if (failures.length > 0) {
    console.error(failures.map((failure) => `FAIL ${failure}`).join('\n'));
    process.exit(1);
}
console.log(`encryption check passed: ${SAMPLES.length} files, each opened with qpdf under its passwords and decrypted to the unencrypted render`);
