// Opens the PDF the core writes with files in it with qpdf, an implementation of the PDF standard that shares no
// code with ours, and checks what a reader would see:
//
//  - the file passes `qpdf --check`;
//  - `qpdf --list-attachments` lists each file the fixture carries, under its name, with its MIME type and its date;
//  - `qpdf --show-attachment` gives back the bytes of each file as they were loaded.
//
// The sample comes from the core's own tests, which write the PDF and, in `files/`, each file as it was loaded:
//   LPDF_ATTACHMENTS_OUT=<folder> cargo test --manifest-path src/core/Cargo.toml --lib writes_attachment_samples_for_the_external_check
//
// Usage: node scripts/check-attachments.mjs <folder>     list the failures, exit 1 if any
//        needs qpdf on the PATH (`apt install qpdf`, `brew install qpdf`)

import { spawnSync } from 'node:child_process';
import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';

const folder = process.argv[2];
if (!folder) {
    console.error('usage: node scripts/check-attachments.mjs <folder of samples>');
    process.exit(2);
}

/** The files the fixture carries, with the MIME type each must report. */
const FILES = [
    { name: 'invoice.xml', mime: 'text/xml' },
    { name: 'terms.txt', mime: 'text/plain' },
];

const pdf = path.join(folder, 'attachments.pdf');
const failures = [];

/** Runs qpdf; a non-zero exit is data here, not an error. Output is bytes, so a file comes back as it is. */
function qpdf(...args) {
    const run = spawnSync('qpdf', args, { maxBuffer: 64 * 1024 * 1024 });
    if (run.error) {
        console.error(`could not run qpdf: ${run.error.message}`);
        process.exit(2);
    }
    return { status: run.status, stdout: run.stdout, output: `${run.stdout}${run.stderr}` };
}

function expect(ok, message) {
    if (!ok) failures.push(message);
}

const checked = qpdf('--check', pdf);
expect(checked.status === 0, `qpdf --check failed: ${checked.output.trim()}`);

const listed = qpdf('--list-attachments', '--verbose', pdf);
expect(listed.status === 0, `qpdf --list-attachments failed: ${listed.output.trim()}`);
const loaded = readdirSync(path.join(folder, 'files'));
expect(
    loaded.length === FILES.length && FILES.every((file) => loaded.includes(file.name)),
    `the sample's files are ${loaded.join(', ')}, not ${FILES.map((file) => file.name).join(', ')}`,
);

for (const file of FILES) {
    // Each entry starts at the start of a line with its key, and its details follow, indented.
    const entry = listed.output.split(/\n(?=\S)/).find((block) => block.startsWith(`${file.name} -> `));
    if (!entry) {
        failures.push(`${file.name}: not listed by qpdf --list-attachments`);
        continue;
    }
    expect(entry.includes(`mime type: ${file.mime}`), `${file.name}: qpdf does not report the MIME type ${file.mime}`);
    expect(/modification date: D:\d{8}/.test(entry), `${file.name}: qpdf reports no modification date`);

    const shown = qpdf(`--show-attachment=${file.name}`, pdf);
    const expected = readFileSync(path.join(folder, 'files', file.name));
    expect(shown.status === 0, `${file.name}: qpdf --show-attachment failed: ${shown.output.trim()}`);
    expect(Buffer.compare(shown.stdout, expected) === 0, `${file.name}: the bytes qpdf gives back differ from the file loaded`);
}

if (failures.length > 0) {
    console.error(failures.map((failure) => `FAIL ${failure}`).join('\n'));
    process.exit(1);
}
console.log(`attachment check passed: ${FILES.length} files listed by qpdf with their types and dates, each given back whole`);
