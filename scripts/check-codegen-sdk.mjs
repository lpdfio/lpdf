// Checks that the code the code generator writes for an SDK's language builds the document it was written from.
//
// The generator (`codegen_wasm`) writes code for VS Code's "generate code" command, the code tabs of the docs and the
// website demo, in JS, C#, PHP, Python and Java. For each document of the checkout it writes the code for the SDK's
// language, builds that document with the SDK, renders it and the XML through the same SDK engine, with a fixed
// creation date and encryption skipped, and requires the two PDFs to be byte-equal. One engine renders both, so the
// generator, the builders and the JSON path are the only difference. The docs' `check-codegen.mjs` does it for JS and
// the examples of the docs.
//
// The documents are the fixtures in `test/fixtures` and the corpus in `test/codegen-corpus.json`, which holds a document
// for every attribute of the schema and every value of an enumeration (`attribute_effects`, in core). The fonts,
// images, files and color profiles a document names are loaded under their names, from the `src` when the checkout has
// the file and with placeholder bytes when not (lib/codegen-check/assets.mjs), so no SDK reads a `src`.
//
// JS, PHP and Python run the code of each document. C# and Java compile every document into one program, a class of
// its own for each, and run it. What each language needs is in lib/codegen-check/<language>.mjs.
//
// The generator can't write a document that binds data (`data-source`, `data-value`), whose code has a `TODO` in place
// of the binding. Such a document fails the check unless it is in CANNOT_EXPRESS below with the reason, and an entry
// fails once the generator can write the document.
//
// An SDK's CI fetches this script and the documents at the core tag it builds against, as it does gen-sdk-api.mjs, and
// the engine files that hold `codegen_wasm`, `lpdf.js` and `lpdf_bg.wasm`, which every release candidate and release
// publishes:
//
//   node _check/scripts/check-codegen-sdk.mjs --sdk java=. --engine _engine
//
// Usage: node scripts/check-codegen-sdk.mjs                          the five SDKs under repos/, with the engine of dist/node
//        node scripts/check-codegen-sdk.mjs --sdk node=.             only the SDK in the given folder (an SDK repository's CI)
//        node scripts/check-codegen-sdk.mjs --engine <folder>        the folder with lpdf.js and lpdf_bg.wasm (default: dist/node)
//        node scripts/check-codegen-sdk.mjs --docker <image>         run PHP or Python in an image that has the SDK in /app (local runs)
//                                                                    --mount <folder>:<path> adds a folder to the container, repeatable
//        node scripts/check-codegen-sdk.mjs --only <text>            only the documents whose name has the text
//        node scripts/check-codegen-sdk.mjs --verbose                also list every document that passed
//        node scripts/check-codegen-sdk.mjs --keep                   keep the folder with the programs
//
// It exits 1 when a document fails, and 2 when it can't run.

import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { CREATED_ON, writePlaceholders } from './lib/codegen-check/assets.mjs';
import { firstLine } from './lib/codegen-check/common.mjs';
import { sdkRepositories } from './lib/sdk-repositories.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const argv = process.argv.slice(2);

/** Why the generator can't write the code of a document, by the name the report gives it. */
const DATA_BINDING = 'it binds data with data-source or data-value, which the builders of the SDKs can not write';
const CANNOT_EXPRESS = new Map([
    ...[
        'bench_data_m', 'bench_data_s', 'data-invoice', 'showcase-bulk', 'showcase-columns-data', 'showcase-markdown-styles',
        'showcase-page-letters', 'showcase-page-numbers', 'showcase-pdfua-toc', 'showcase-pdfua', 'showcase-references',
        'showcase-toc',
    ].map((name) => [`fixtures/${name}.xml`, DATA_BINDING]),
    ...[
        'section@type=toc', 'section@toc-indent-step=xs', 'column@bulk=true', 'column@bulk=false', 'list@bulk=true',
        'list@bulk=false', 'grid@bulk=true', 'grid@bulk=false', 'table@bulk=true', 'table@bulk=false',
    ].map((id) => [`corpus/${id}`, DATA_BINDING]),
]);

/** What the generator writes in place of code it can't write: a TODO comment, or a call of a builder no SDK has. */
const UNWRITTEN = /TODO \(Lpdf\)|\bL(?:\.|::)[eE]lement\(/;

function fail(message) {
    console.error(message);
    process.exit(2);
}

/** The value of `--name <value>` or `--name=<value>`. */
function option(name) {
    const at = argv.findIndex((argument) => argument === name || argument.startsWith(`${name}=`));
    if (at < 0) return undefined;
    return argv[at].includes('=') ? argv[at].slice(name.length + 1) : argv[at + 1];
}

/** Every value of a repeated `--name <value>`. */
function optionValues(name) {
    return argv.flatMap((argument, at) => (argument === name ? [argv[at + 1]] : argument.startsWith(`${name}=`) ? [argument.slice(name.length + 1)] : []));
}

const engineFolder = path.resolve(option('--engine') ?? path.join(root, 'dist/node'));
for (const file of ['lpdf.js', 'lpdf_bg.wasm']) {
    if (!existsSync(path.join(engineFolder, file))) fail(`${file} is not in ${engineFolder}: give the folder with the engine files with --engine`);
}
const engine = createRequire(import.meta.url)(path.join(engineFolder, 'lpdf.js'));

const only = option('--only');
const docker = option('--docker') ? { image: option('--docker'), mounts: optionValues('--mount') } : undefined;
const verbose = argv.includes('--verbose');
const keep = argv.includes('--keep');
const { repositories, selected } = sdkRepositories(root, argv);

/** The fixtures and the corpus, each with the name the report gives it. */
function loadDocuments() {
    const fixtures = path.join(root, 'test/fixtures');
    const corpusFile = path.join(root, 'test/codegen-corpus.json');
    if (!existsSync(fixtures) || !existsSync(corpusFile)) fail(`test/fixtures and test/codegen-corpus.json are not in ${root}`);
    const documents = [
        ...readdirSync(fixtures)
            .filter((name) => name.endsWith('.xml'))
            .sort()
            .map((name) => ({ name: `fixtures/${name}`, xml: readFileSync(path.join(fixtures, name), 'utf8') })),
        ...JSON.parse(readFileSync(corpusFile, 'utf8')).documents.map(({ id, xml }) => ({ name: `corpus/${id}`, xml })),
    ];
    return documents.map((document, index) => ({ ...document, index }));
}

const REASONS = {
    differs: 'the code renders another PDF than the XML',
    xml: 'the XML does not render',
    code: 'the code does not build or render',
    shape: 'the code is not in the shape the check takes apart',
    compile: 'the code does not compile',
};

/** Checks the SDK of one language; returns the number of documents that failed. */
async function checkLanguage(language, sdk, all) {
    const runner = await import(`./lib/codegen-check/${language}.mjs`);
    const failures = [];
    const left = [];
    const documents = [];
    for (const document of all) {
        if (only && !document.name.includes(only)) continue;
        let code;
        try {
            code = engine.codegen_wasm(document.xml, JSON.stringify({ target: runner.target }));
        } catch (e) {
            failures.push(`${document.name}: the generator refuses it: ${firstLine(e)}`);
            continue;
        }
        const excuse = CANNOT_EXPRESS.get(document.name);
        const unwritten = UNWRITTEN.test(code);
        if (unwritten && excuse) {
            left.push(document.name);
        } else if (unwritten) {
            failures.push(`${document.name}: the generator can't write it (${firstLine(code.match(new RegExp(`.*${UNWRITTEN.source}.*`))?.[0])}): fix it, or list it in CANNOT_EXPRESS with the reason`);
        } else if (excuse) {
            failures.push(`${document.name}: listed in CANNOT_EXPRESS (${excuse}) and the generator writes it now: remove it from the list`);
        } else {
            documents.push({ ...document, code });
        }
    }

    const work = mkdtempSync(path.join(tmpdir(), `lpdf-codegen-${language}-`));
    const placeholders = writePlaceholders(root, path.join(work, 'assets'));
    const started = Date.now();
    const results = await runner.run({ sdk, work, base: path.join(root, 'test/fixtures'), placeholders, documents, createdOn: CREATED_ON, docker });

    let passed = 0;
    for (const { name, index } of documents) {
        const result = results.get(index) ?? { status: 'code', message: 'the program printed no result' };
        if (result.status === 'ok') {
            passed++;
            if (verbose) console.log(`ok    ${name}`);
        } else {
            failures.push(`${name}: ${REASONS[result.status] ?? result.status}${result.message ? `: ${result.message}` : ''}`);
        }
    }
    for (const name of left) console.log(`left  ${language}  ${name}: ${CANNOT_EXPRESS.get(name)}`);

    for (const message of failures) console.error(`FAIL  ${language}  ${message}`);
    const seconds = Math.round((Date.now() - started) / 1000);
    console.log(`${language}: ${passed} of ${documents.length} documents render the PDF of their XML, ${failures.length} failed, ${left.length} left out (${seconds} s)`);
    if (failures.length === 0 && !keep) rmSync(work, { recursive: true, force: true });
    else console.log(`${language}: the programs are in ${work}`);
    return failures.length;
}

const documents = loadDocuments();
let failed = 0;
for (const language of selected) failed += await checkLanguage(language, repositories[language], documents);
process.exit(failed > 0 ? 1 : 0);
