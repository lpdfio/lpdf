// What the runners of scripts/check-codegen-sdk.mjs share.
//
// A runner takes the documents to check and returns what became of each, by the document's index:
//
//   ok        the code and the XML rendered the same PDF
//   differs   both rendered, and the PDFs are not the same
//   xml       the XML did not render
//   code      the code did not build, or did not render
//   shape     the code the generator wrote is not in the shape the runner takes it apart by
//   compile   the code does not compile (.NET and Java)
//
// A runner that starts a program of the SDK's language has the program print a line for each document, which
// `readResults` reads back:
//
//   RESULT <tab> index <tab> status <tab> message

import { spawnSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';

import { declaredAssets } from './assets.mjs';

/** The first line of a message, cut to a length that fits a report. */
export function firstLine(message, limit = 300) {
    const line = String(message ?? '').split('\n').map((text) => text.trim()).find((text) => text) ?? '';
    return line.length > limit ? `${line.slice(0, limit)}…` : line;
}

/** Runs a command and returns its exit status and output; one that can't start is the check's error. */
export function run(command, args, options = {}) {
    const result = spawnSync(command, args, { encoding: 'utf8', maxBuffer: 1024 * 1024 * 1024, ...options });
    if (result.error) {
        console.error(`could not run ${command}: ${result.error.message}`);
        process.exit(2);
    }
    return { status: result.status, stdout: result.stdout, stderr: result.stderr };
}

/**
 * Writes each document's XML to `docs/<index>.xml` in `work` and the assets it declares to `docs/<index>.assets`, a
 * line for each: the kind, the key to load it under and the path of the file to load there, separated by tabs. The
 * programs read both, so no document is embedded in one, where a string has a limit in some languages.
 */
export function writeDocuments(work, documents, base, placeholders) {
    mkdirSync(path.join(work, 'docs'), { recursive: true });
    for (const { index, xml } of documents) {
        writeFileSync(path.join(work, 'docs', `${index}.xml`), xml);
        const lines = declaredAssets(xml, base, placeholders).map(({ kind, key, path: file }) => `${kind}\t${key}\t${file}`);
        writeFileSync(path.join(work, 'docs', `${index}.assets`), lines.length ? `${lines.join('\n')}\n` : '');
    }
}

/** The results a program printed, by index. A document the program printed nothing for has none. */
export function readResults(output) {
    const results = new Map();
    for (const line of output.split('\n')) {
        const [tag, index, status, ...message] = line.replace(/\r$/, '').split('\t');
        if (tag === 'RESULT') results.set(Number(index), { status, message: message.join(' ') });
    }
    return results;
}

/** The result of every document in `documents` that `results` has nothing for, as the program's own failure. */
export function missingResults(results, documents, output) {
    for (const { index } of documents) {
        if (!results.has(index)) {
            results.set(index, { status: 'code', message: `the program ended without a result: ${firstLine(output)}` });
        }
    }
    return results;
}
