// Validates the Factur-X invoices the core writes with the Mustang CLI, the open checker of Factur-X and ZUGFeRD,
// which shares no code with ours. It runs veraPDF inside it, checks the four fx: properties of the XMP packet and finds
// the invoice's XML, then checks that XML against the rules of its profile:
//
//  - the PDF part of each sample's report must be valid: PDF/A-3, and fx:DocumentType, fx:DocumentFileName,
//    fx:Version and fx:ConformanceLevel. A PDF the Mustang CLI calls invalid fails the check, with its errors;
//  - the XML part is shown and does not fail it: the fixtures' XML are small samples of our own, and the content of
//    the XML is the caller's to check, not the engine's;
//  - a copy of the EN 16931 sample with a wrong fx:ConformanceLevel, which this script writes, must fail it: a check
//    that passes that copy checks nothing.
//
// The Mustang CLI doesn't read the extension schema that describes the fx: properties: it passed a wrong namespace
// there. facturx_render_tests.rs compares that block with the spec's.
//
// The samples come from the core's own tests:
//   LPDF_FACTURX_OUT=<folder> cargo test --manifest-path src/core/Cargo.toml --lib writes_facturx_samples_for_the_external_check
//
// Usage: node scripts/check-facturx.mjs <folder of samples>     list the failures, exit 1 if any. Needs Java 11 or
//        later on the PATH. The Mustang CLI of MUSTANG_VERSION is downloaded from Maven Central into target/mustang
//        once, and a file whose SHA-256 is not MUSTANG_SHA256 is refused.

import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';

const MUSTANG_VERSION = '2.26.0';
const MUSTANG_SHA256 = '42d7868cb68264874a7b8cab4c3587b03b23ccc7cd72373da917f66758bb9736';
const MUSTANG_URL = `https://repo1.maven.org/maven2/org/mustangproject/Mustang-CLI/${MUSTANG_VERSION}/Mustang-CLI-${MUSTANG_VERSION}.jar`;
const JAR = path.join('target', 'mustang', `Mustang-CLI-${MUSTANG_VERSION}.jar`);

/** The sample the check of the check copies, and the change that makes its fx:ConformanceLevel wrong. */
const ALTERED_FROM = 'en16931.pdf';
const ALTERED = 'en16931-wrong-conformance-level.pdf';
const RIGHT_LEVEL = '<fx:ConformanceLevel>EN 16931</fx:ConformanceLevel>';
const WRONG_LEVEL = '<fx:ConformanceLevel>EN 16939</fx:ConformanceLevel>';

const [folder] = process.argv.slice(2);
if (!folder) {
    console.error('usage: node scripts/check-facturx.mjs <folder of samples>');
    process.exit(2);
}

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

/** The Mustang CLI's jar, downloaded once and refused when its bytes are not the pinned ones. */
async function mustang() {
    if (!existsSync(JAR)) {
        mkdirSync(path.dirname(JAR), { recursive: true });
        const response = await fetch(MUSTANG_URL);
        if (!response.ok) {
            console.error(`could not download ${MUSTANG_URL}: ${response.status} ${response.statusText}`);
            process.exit(2);
        }
        writeFileSync(JAR, Buffer.from(await response.arrayBuffer()));
    }
    const actual = sha256(readFileSync(JAR));
    if (actual !== MUSTANG_SHA256) {
        console.error(`${JAR} has SHA-256 ${actual}, not the pinned ${MUSTANG_SHA256}: delete it and run again`);
        process.exit(2);
    }
    return JAR;
}

/** The text between `<tag>` and `</tag>` in `xml`, or '' when it has none. */
function section(xml, tag) {
    return xml.match(new RegExp(`<${tag}>([\\s\\S]*?)</${tag}>`))?.[1] ?? '';
}

/** The status of the last `<summary>` of a part of the report: `valid` or `invalid`. */
function status(part) {
    return [...part.matchAll(/<summary\s+status=["'](\w+)["']\s*\/>/g)].at(-1)?.[1];
}

/** The errors of a part of the report, each with its type: `12: XMP Metadata: ConformanceLevel contains invalid value`. */
function errors(part) {
    return [...part.matchAll(/<(error|exception)(?:\s+type=["'](\d+)["'])?[^>]*>([\s\S]*?)<\/\1>/g)]
        .map(([, , type, message]) => `${type ?? '-'}: ${message.trim().split('\n')[0]}`);
}

/** The Mustang CLI's report on `file`: the PDF part's status and errors, and the XML part's. */
function validate(jar, file) {
    const result = spawnSync(
        'java',
        ['-jar', jar, '--action', 'validate', '--source', file, '--no-notices', '--disable-file-logging'],
        { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 },
    );
    if (result.error) {
        console.error(`could not run java: ${result.error.message}`);
        process.exit(2);
    }
    // The CLI exits non-zero when the XML part is invalid: the report says which part is.
    const report = `${result.stdout}`.match(/<validation[\s\S]*<\/validation>/)?.[0];
    if (!report) return { error: `${result.stdout}${result.stderr}`.trim().split('\n').slice(-5).join(' / ') };
    const pdf = section(report, 'pdf');
    const xml = section(report, 'xml');
    return { pdf: { status: status(pdf), errors: errors(pdf) }, xml: { status: status(xml), errors: errors(xml) } };
}

const jar = await mustang();
const java = spawnSync('java', ['-version'], { encoding: 'utf8' });
console.log(`Mustang CLI ${MUSTANG_VERSION} on ${(java.stderr || java.stdout || '').split('\n')[0]}`);

const samples = readdirSync(folder).filter((name) => name.endsWith('.pdf') && name !== ALTERED).sort();
if (!samples.includes(ALTERED_FROM)) {
    console.error(`${folder} has no ${ALTERED_FROM}: write the samples first`);
    process.exit(2);
}
const original = readFileSync(path.join(folder, ALTERED_FROM));
const altered = Buffer.from(original.toString('latin1').replace(RIGHT_LEVEL, WRONG_LEVEL), 'latin1');
if (altered.equals(original)) {
    console.error(`${ALTERED_FROM} has no ${RIGHT_LEVEL} to change`);
    process.exit(2);
}
writeFileSync(path.join(folder, ALTERED), altered);

const failures = [];
const lines = [];
for (const sample of samples) {
    const report = validate(jar, path.join(folder, sample));
    if (report.error) {
        failures.push(`${sample}: the Mustang CLI gave no report: ${report.error}`);
        continue;
    }
    lines.push(`${sample}: PDF ${report.pdf.status}, XML ${report.xml.status}${report.xml.errors.length ? ` (${report.xml.errors.length} errors in the sample's own XML)` : ''}`);
    if (report.pdf.status !== 'valid') {
        failures.push(`${sample}: the Mustang CLI calls the PDF ${report.pdf.status ?? 'unknown'}: ${report.pdf.errors.join('; ') || 'no error given'}`);
    }
}

const check = validate(jar, path.join(folder, ALTERED));
if (check.error || check.pdf.status !== 'invalid' || !check.pdf.errors.some((error) => error.startsWith('12:'))) {
    failures.push(
        `${ALTERED}: a wrong fx:ConformanceLevel must make the PDF invalid with error 12, and the Mustang CLI gave ` +
            `${check.error ?? `${check.pdf.status}: ${check.pdf.errors.join('; ') || 'no error'}`}`,
    );
} else {
    lines.push(`${ALTERED}: PDF invalid, as it must be (${check.pdf.errors.join('; ')})`);
}

console.log(lines.join('\n'));
if (failures.length > 0) {
    console.error(failures.map((failure) => `FAIL ${failure}`).join('\n'));
    process.exit(1);
}
console.log(`Factur-X check passed: the Mustang CLI ${MUSTANG_VERSION} calls the PDF of the ${samples.length} samples valid, and the altered copy invalid`);
