// Writes THIRD_PARTY_LICENSES_ENGINE.txt: the Rust crates the engine links, each with its license and the license text,
// which the MIT, BSD and Apache-2.0 licenses ask to travel with compiled code. Every package that ships the engine
// carries the file, core's release attaches it, and the docs site serves it.
//
// The crates are those the engine's shipped builds link (BUILDS), normal dependencies only: `Cargo.lock` also holds the
// test and benchmark tools, which end up in no build, and a proc-macro crate runs when the engine is compiled and is
// in no build either. The texts are the license and NOTICE files of each crate's package, read from the Cargo cache,
// each text written once with the crates that carry it.
//
// A license that isn't in ACCEPTED fails the run, so a new dependency under another license is a decision someone
// makes. A crate whose package has no license file needs an entry in OVERRIDES.
//
// Usage: node scripts/third-party-licenses.mjs            writes the file
//        node scripts/third-party-licenses.mjs --check    exits 1 when the file isn't what this would write

import { execFileSync } from 'node:child_process';
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const ROOT = path.join(import.meta.dirname, '..');
export const OUTPUT_NAME = 'THIRD_PARTY_LICENSES_ENGINE.txt';

/** The builds the packages ship: a crate of the workspace and the target it is built for. */
export const BUILDS = [
    // The JS SDK and the VS Code extension.
    { crate: 'lpdf', target: 'wasm32-unknown-unknown' },
    // The .NET, Java and Python SDKs, and the PHP SDK where it runs the wasmtime program.
    { crate: 'lpdf-wasi', target: 'wasm32-wasip1' },
    // The native library of the PHP SDK.
    { crate: 'lpdf-ffi', target: 'x86_64-unknown-linux-gnu' },
    { crate: 'lpdf-ffi', target: 'aarch64-unknown-linux-gnu' },
];

/** The licenses the engine may link, by SPDX identifier. Another one is added here on purpose, after reading it. */
export const ACCEPTED = [
    '0BSD',
    'Apache-2.0',
    'BSD-2-Clause',
    'BSD-3-Clause',
    'BSL-1.0',
    'MIT',
    'Unicode-3.0',
    'Unlicense',
    'Zlib',
];

/**
 * The license text of a crate whose package carries none: `name version` to a file of `third-party-licenses/`, the
 * text of the crate's repository. A new version of the crate is read again, since its license may have changed.
 */
export const OVERRIDES = {
    'alloc-stdlib 0.2.4': 'alloc-stdlib-0.2.4.LICENSE',
};

const OVERRIDES_DIR = path.join(import.meta.dirname, 'third-party-licenses');
const LICENSE_FILE = /^(licen[cs]e|copying|unlicense|notice)([-._].*)?$/i;
const WIDTH = 80;

/** A license problem, which fails the run. */
export class LicenseError extends Error
{
    constructor(message)
    {
        super(message);
        this.name = 'LicenseError';
    }
}

/**
 * Whether an SPDX license expression is met by the accepted licenses: an `OR` by any one of its alternatives, an `AND`
 * by all of them. `A WITH exception` is met by `A`, since an exception only adds permissions. The old `A/B` of
 * Cargo.toml means `A OR B`.
 */
export function acceptsLicense(expression, accepted)
{
    const tokens = expression.replace(/\s*\/\s*/g, ' OR ').match(/[()]|[^\s()]+/g) ?? [];
    let at = 0;
    const isOperator = (word) => tokens[at]?.toUpperCase() === word;

    const parseOr = () => {
        let result = parseAnd();
        while (isOperator('OR'))
        {
            at++;
            const next = parseAnd();
            result = result || next;
        }
        return result;
    };
    const parseAnd = () => {
        let result = parseTerm();
        while (isOperator('AND'))
        {
            at++;
            const next = parseTerm();
            result = result && next;
        }
        return result;
    };
    const parseTerm = () => {
        const token = tokens[at++];
        if (token === undefined || token === ')')
        {
            throw new LicenseError(`The license expression "${expression}" ends where a license was expected`);
        }
        if (token === '(')
        {
            const inner = parseOr();
            if (tokens[at++] !== ')')
            {
                throw new LicenseError(`The license expression "${expression}" has a parenthesis that isn't closed`);
            }
            return inner;
        }
        if (isOperator('WITH')) at += 2;
        return accepted.includes(token);
    };

    const result = parseOr();
    if (at !== tokens.length)
    {
        throw new LicenseError(`The license expression "${expression}" has "${tokens[at]}" where an operator was expected`);
    }
    return result;
}

/**
 * The crates a build links, `name version` each, from the output of
 * `cargo tree -p <crate> --target <target> -e normal --prefix depth --no-dedupe --format '{p}'`. That is the tree of
 * the build's own features, which `cargo metadata` can't give: it unifies the features of every crate of the
 * workspace, the test and benchmark tools' too, and so names crates that no build links. A proc-macro crate and what
 * only it needs are left out, since they run when the engine is compiled and are in no build. `--no-dedupe` writes a
 * crate again under each crate that needs it, so one that a proc-macro crate shares with a linked crate stays.
 */
export function linkedCrates(treeOutput)
{
    const linked = new Set();
    let skipBelow = Infinity;
    for (const line of treeOutput.split('\n'))
    {
        const match = /^(\d+)(\S+) v(\S+)(.*)$/.exec(line);
        if (!match) continue;
        const depth = Number(match[1]);
        if (depth > skipBelow) continue;
        skipBelow = Infinity;
        if (match[4].includes('(proc-macro)'))
        {
            skipBelow = depth;
            continue;
        }
        linked.add(`${match[2]} ${match[3]}`);
    }
    return [...linked];
}

/** A text as it is compared and written: the same lines on every system, with no space at the end of a line. */
export function normalizeText(raw)
{
    return raw
        .replace(/^\uFEFF/, '')
        .replace(/\r\n?/g, '\n')
        .replace(/[ \t]+$/gm, '')
        .trim();
}

/** The license and NOTICE files in a package's folder, by name. */
export function licenseFiles(directory)
{
    return readdirSync(directory, { withFileTypes: true })
        .filter((entry) => entry.isFile() && LICENSE_FILE.test(entry.name))
        .map((entry) => entry.name)
        .sort(compare)
        .map((name) => ({
            name,
            kind: /^notice/i.test(name) ? 'notice' : 'license',
            text: normalizeText(readFileSync(path.join(directory, name), 'utf8')),
        }));
}

/**
 * Reads each package's license and texts. A package whose license isn't accepted, one with no license expression and
 * one with no text, and an override that matches none, come back as problems.
 */
export function collectCrates(packages, { accepted = ACCEPTED, overrides = OVERRIDES, overridesDir = OVERRIDES_DIR } = {})
{
    const problems = [];
    const usedOverrides = new Set();
    const crates = [];

    for (const pkg of [...packages].sort(compareCrates))
    {
        const label = `${pkg.name} ${pkg.version}`;
        if (!pkg.license)
        {
            problems.push(`${label} names no license expression in its Cargo.toml`);
        }
        else if (!acceptsLicense(pkg.license, accepted))
        {
            problems.push(`${label} is licensed "${pkg.license}", which none of the accepted licenses meets: read it, and `
                + 'add its identifier to ACCEPTED in scripts/third-party-licenses.mjs if it may ship');
        }

        let files = licenseFiles(path.dirname(pkg.manifest_path));
        if (!files.length && overrides[label])
        {
            usedOverrides.add(label);
            const text = normalizeText(readFileSync(path.join(overridesDir, overrides[label]), 'utf8'));
            files = [{ name: overrides[label], kind: 'license', text }];
        }
        if (!files.length)
        {
            problems.push(`${label} has no license file in its package: add its text to scripts/third-party-licenses/ `
                + 'and to OVERRIDES in scripts/third-party-licenses.mjs');
        }
        crates.push({ name: pkg.name, version: pkg.version, license: pkg.license, repository: pkg.repository, files });
    }

    for (const label of Object.keys(overrides).filter((key) => !usedOverrides.has(key)))
    {
        problems.push(`OVERRIDES has an entry for ${label}, which no linked crate without a license file matches: remove it`);
    }
    return { crates, problems };
}

/** The text of the file: the crates, then each distinct text once, with the crates that carry it. */
export function render(crates)
{
    const sorted = [...crates].sort(compareCrates);
    const texts = new Map();
    for (const crate of sorted)
    {
        for (const file of crate.files)
        {
            const key = `${file.kind}\n${file.text}`;
            if (!texts.has(key)) texts.set(key, { kind: file.kind, text: file.text, users: [] });
            const label = `${crate.name} ${crate.version}`;
            const users = texts.get(key).users;
            if (users.at(-1) !== label) users.push(label);
        }
    }

    const rule = (character) => character.repeat(WIDTH - 1);
    const lines = [
        'THIRD-PARTY LICENSES OF THE LPDF ENGINE',
        rule('='),
        '',
        'The Lpdf engine is written in Rust and compiled into the WebAssembly',
        'modules and the native library that the Lpdf packages carry. The crates',
        'below are linked into those builds. Each is the work of its authors and is',
        'used under the license named beside it. The license texts, with the',
        'copyright notices of the authors, follow the list.',
        '',
        'This file is generated from the dependencies of the engine by',
        'scripts/third-party-licenses.mjs. Do not edit it by hand.',
        '',
        '',
        `CRATES (${sorted.length})`,
        rule('='),
        '',
    ];
    for (const crate of sorted)
    {
        lines.push([`${crate.name} ${crate.version}`, crate.license, crate.repository].filter(Boolean).join(' | '));
    }
    lines.push('', '', `LICENSE TEXTS (${texts.size})`, rule('='));
    for (const { kind, text, users } of texts.values())
    {
        lines.push('', kind === 'notice' ? 'NOTICE of:' : 'License text of:', ...wrapList(users, '    '), rule('-'), text, '');
    }
    return `${lines.join('\n').replace(/\n+$/, '')}\n`;
}

/** Items joined by commas and broken into lines of at most WIDTH characters, each starting with `indent`. */
function wrapList(items, indent)
{
    const lines = [];
    let line = indent;
    items.forEach((item, index) => {
        const piece = index < items.length - 1 ? `${item},` : item;
        if (line !== indent && line.length + 1 + piece.length > WIDTH)
        {
            lines.push(line);
            line = indent;
        }
        line += line === indent ? piece : ` ${piece}`;
    });
    lines.push(line);
    return lines;
}

function compare(a, b)
{
    if (a < b) return -1;
    return a > b ? 1 : 0;
}

function compareCrates(a, b)
{
    return compare(a.name, b.name) || compare(a.version, b.version);
}

function cargo(args)
{
    return execFileSync('cargo', args, { cwd: ROOT, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024 });
}

function main()
{
    // The packages of the lock file, for each one's license, repository and folder. The crates of this workspace have
    // no `source` and are not listed.
    const metadata = JSON.parse(cargo(['metadata', '--format-version', '1', '--locked']));
    const external = new Map(metadata.packages.filter((pkg) => pkg.source).map((pkg) => [`${pkg.name} ${pkg.version}`, pkg]));

    const linked = new Map();
    for (const { crate, target } of BUILDS)
    {
        const tree = cargo(['tree', '--locked', '-p', crate, '--target', target, '-e', 'normal', '--prefix', 'depth',
            '--no-dedupe', '--format', '{p}']);
        for (const label of linkedCrates(tree))
        {
            if (external.has(label)) linked.set(label, external.get(label));
        }
    }

    const { crates, problems } = collectCrates([...linked.values()]);
    if (problems.length)
    {
        for (const problem of problems) console.error(problem);
        process.exit(1);
    }

    const text = render(crates);
    const file = path.join(ROOT, OUTPUT_NAME);
    if (process.argv.includes('--check'))
    {
        let current = '';
        try
        {
            current = readFileSync(file, 'utf8').replace(/\r\n/g, '\n');
        }
        catch (error)
        {
            if (error.code !== 'ENOENT') throw error;
        }
        if (current !== text)
        {
            console.error(`${OUTPUT_NAME} isn't what the engine's dependencies give: run make gen-third-party-licenses `
                + 'and commit the file');
            process.exit(1);
        }
        console.log(`${OUTPUT_NAME} holds the ${crates.length} crates the engine links`);
        return;
    }
    writeFileSync(file, text);
    console.log(`Wrote ${OUTPUT_NAME}: ${crates.length} crates`);
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) main();
