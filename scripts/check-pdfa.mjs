// Validates the PDF/A and PDF/UA files the core writes with veraPDF, the PDF Association's reference validator, which
// shares no code with ours, and checks their structure with qpdf:
//
//  - veraPDF reads the standards each file declares in its XMP packet and validates the file against each one's
//    rules. For PDF/A: pdfa-2b.pdf must declare PDF/A-2b, and pdfa-2u.pdf and the three pages in a font of each kind
//    the fixtures lack (CFF, CFF2, variable TrueType) PDF/A-2u, and so must pdfa-2u-ocg.pdf, the fixture with content
//    in optional content groups. For PDF/UA: pdfua-1.pdf must declare PDF/UA-1, and pdfua-1-pdfa-2u.pdf both PDF/A-2u
//    and PDF/UA-1, and pdfua-1-toc-links.pdf, a table of contents of the links $toc repeats, pdfua-1-toc.pdf, the
//    fixture with a table of contents on two pages, and pdfua-1-ocg.pdf, the fixture with content in optional content
//    groups, PDF/UA-1. None may break a rule;
//  - qpdf --check finds no syntax or stream error.
//
// The samples come from the core's own tests:
//   LPDF_PDFA_OUT=<folder> cargo test --manifest-path src/core/Cargo.toml --lib writes_pdfa_samples_for_the_external_check
//   LPDF_PDFUA_OUT=<folder> cargo test --manifest-path src/core/Cargo.toml --lib writes_pdfua_samples_for_the_external_check
//
// Usage: node scripts/check-pdfa.mjs <folder> [pdfa|pdfua]     the samples of PDF/A (the default) or of PDF/UA; list
//        the failures, exit 1 if any. Needs Docker, which runs veraPDF's image (VERAPDF_IMAGE, verapdf/cli:v1.30.2 by
//        default), and qpdf on the PATH (`apt install qpdf`, `brew install qpdf`)

import { spawnSync } from 'node:child_process';
import path from 'node:path';

const [folder, standard = 'pdfa'] = process.argv.slice(2);
/** The samples each test writes, and the profiles veraPDF must find in each. */
const SAMPLE_SETS = {
    // writes_pdfa_samples_for_the_external_check
    pdfa: [
        { file: 'pdfa-2b.pdf', profiles: ['PDF/A-2b validation profile'] },
        { file: 'pdfa-2u.pdf', profiles: ['PDF/A-2u validation profile'] },
        { file: 'pdfa-2u-cff.pdf', profiles: ['PDF/A-2u validation profile'] },
        { file: 'pdfa-2u-cff2.pdf', profiles: ['PDF/A-2u validation profile'] },
        { file: 'pdfa-2u-variable-ttf.pdf', profiles: ['PDF/A-2u validation profile'] },
        { file: 'pdfa-2u-ocg.pdf', profiles: ['PDF/A-2u validation profile'] },
    ],
    // writes_pdfua_samples_for_the_external_check
    pdfua: [
        { file: 'pdfua-1.pdf', profiles: ['PDF/UA-1 validation profile'] },
        { file: 'pdfua-1-pdfa-2u.pdf', profiles: ['PDF/A-2u validation profile', 'PDF/UA-1 validation profile'] },
        { file: 'pdfua-1-toc-links.pdf', profiles: ['PDF/UA-1 validation profile'] },
        { file: 'pdfua-1-toc.pdf', profiles: ['PDF/UA-1 validation profile'] },
        { file: 'pdfua-1-ocg.pdf', profiles: ['PDF/UA-1 validation profile'] },
    ],
};
const SAMPLES = SAMPLE_SETS[standard];
if (!folder || !SAMPLES) {
    console.error('usage: node scripts/check-pdfa.mjs <folder of samples> [pdfa|pdfua]');
    process.exit(2);
}

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
    // A file that declares two standards gets a validation of each.
    const validations = job.validationResult ?? [];
    if (validations.length === 0) {
        failures.push(`${sample.file}: veraPDF gave no validation result: ${job.error ?? JSON.stringify(job.taskException ?? job)}`);
    }
    const declared = validations.map((validation) => validation.profileName).sort();
    if (validations.length > 0 && declared.join(', ') !== [...sample.profiles].sort().join(', ')) {
        failures.push(`${sample.file}: declares ${declared.join(', ')}, expected ${sample.profiles.join(', ')}`);
    }
    sample.checks = 0;
    for (const validation of validations) {
        for (const rule of validation.details.ruleSummaries ?? []) {
            failures.push(
                `${sample.file}: ${validation.profileName} rule ${rule.clause}-${rule.testNumber} failed ${rule.failedChecks} times: ${rule.description}`,
            );
        }
        if (!validation.compliant && (validation.details.ruleSummaries ?? []).length === 0) {
            failures.push(`${sample.file}: veraPDF found it not compliant with ${validation.profileName}`);
        }
        sample.checks += validation.details.passedChecks;
    }

    const checked = run('qpdf', ['--check', file]);
    if (checked.status !== 0) failures.push(`${sample.file}: qpdf --check: ${checked.output.trim()}`);
}

if (failures.length > 0) {
    console.error(failures.map((failure) => `FAIL ${failure}`).join('\n'));
    process.exit(1);
}
const summary = SAMPLES.map((sample) => `${sample.file} (${sample.checks} checks)`).join(', ');
console.log(`${standard === 'pdfua' ? 'PDF/UA' : 'PDF/A'} check passed with ${IMAGE}: ${summary} meet the standards they declare, and qpdf finds no error`);
