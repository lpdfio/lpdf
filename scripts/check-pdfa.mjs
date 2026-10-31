// Validates the PDF/A files the core writes with veraPDF, the PDF Association's reference validator, which shares
// no code with ours, and checks their structure with qpdf:
//
//  - veraPDF reads the level each file declares in its XMP packet and validates the file against that level's
//    rules: pdfa-2b.pdf must declare PDF/A-2b and pdfa-2u.pdf PDF/A-2u, and break none of their rules;
//  - qpdf --check finds no syntax or stream error.
//
// The samples come from the core's own tests:
//   LPDF_PDFA_OUT=<folder> cargo test --manifest-path src/core/Cargo.toml --lib writes_pdfa_samples_for_the_external_check
//
// Usage: node scripts/check-pdfa.mjs <folder>     list the failures, exit 1 if any
//        needs Docker, which runs veraPDF's image (VERAPDF_IMAGE, verapdf/cli:v1.30.2 by default), and qpdf on the
//        PATH (`apt install qpdf`, `brew install qpdf`)

import { spawnSync } from 'node:child_process';
import path from 'node:path';

const folder = process.argv[2];
if (!folder) {
    console.error('usage: node scripts/check-pdfa.mjs <folder of samples>');
    process.exit(2);
}

/** The samples `writes_pdfa_samples_for_the_external_check` writes, and the profile veraPDF must find in each. */
const SAMPLES = [
    { file: 'pdfa-2b.pdf', profile: 'PDF/A-2b validation profile' },
    { file: 'pdfa-2u.pdf', profile: 'PDF/A-2u validation profile' },
];

const IMAGE = process.env.VERAPDF_IMAGE || 'verapdf/cli:v1.30.2';
const failures = [];

/** Runs a command; a non-zero exit is data here, not an error. */
function run(command, args) {
    const result = spawnSync(command, args, { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
    if (result.error) {
        console.error(`could not run ${command}: ${result.error.message}`);
        process.exit(2);
    }
    return { status: result.status, stdout: result.stdout, output: `${result.stdout}${result.stderr}` };
}

/** veraPDF's report on `file`, with the flavour it detects from the file's own XMP packet. */
function verapdf(file) {
    const mount = `${path.resolve(folder)}:/data:ro`;
    const result = run('docker', [
        'run', '--rm', '--platform', 'linux/amd64', '-v', mount, IMAGE, '--flavour', '0', '--format', 'json',
        `/data/${file}`,
    ]);
    try {
        return JSON.parse(result.stdout).report.jobs[0];
    } catch {
        return { error: result.output.trim() };
    }
}

for (const sample of SAMPLES) {
    const file = path.join(folder, sample.file);
    const job = verapdf(sample.file);
    const validation = job.validationResult?.[0];
    if (!validation) {
        failures.push(`${sample.file}: veraPDF gave no validation result: ${job.error ?? JSON.stringify(job.taskException ?? job)}`);
    } else {
        if (validation.profileName !== sample.profile) {
            failures.push(`${sample.file}: declares ${validation.profileName}, expected ${sample.profile}`);
        }
        for (const rule of validation.details.ruleSummaries ?? []) {
            failures.push(
                `${sample.file}: rule ${rule.clause}-${rule.testNumber} failed ${rule.failedChecks} times: ${rule.description}`,
            );
        }
        if (!validation.compliant && (validation.details.ruleSummaries ?? []).length === 0) {
            failures.push(`${sample.file}: veraPDF found it not compliant`);
        }
        sample.checks = validation.details.passedChecks;
    }

    const checked = run('qpdf', ['--check', file]);
    if (checked.status !== 0) failures.push(`${sample.file}: qpdf --check: ${checked.output.trim()}`);
}

if (failures.length > 0) {
    console.error(failures.map((failure) => `FAIL ${failure}`).join('\n'));
    process.exit(1);
}
const summary = SAMPLES.map((sample) => `${sample.file} (${sample.checks} checks)`).join(', ');
console.log(`PDF/A check passed with ${IMAGE}: ${summary} meet the level they declare, and qpdf finds no error`);
