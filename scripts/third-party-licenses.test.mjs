// Tests of third-party-licenses.mjs: node --test scripts/third-party-licenses.test.mjs

import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { test } from 'node:test';

import {
    acceptsLicense,
    collectCrates,
    collectSchemas,
    collectToolchain,
    LicenseError,
    licenseFiles,
    linkedCrates,
    normalizeText,
    render,
    SCHEMAS,
    TOOLCHAIN,
    toolchainLabel,
} from './third-party-licenses.mjs';

const ACCEPTED = ['MIT', 'Apache-2.0', 'BSD-3-Clause', 'Unicode-3.0'];

/** A folder of files, removed when the test ends. */
function withFolder(files, body)
{
    const folder = mkdtempSync(path.join(tmpdir(), 'third-party-licenses-'));
    try
    {
        for (const [name, content] of Object.entries(files))
        {
            mkdirSync(path.dirname(path.join(folder, name)), { recursive: true });
            writeFileSync(path.join(folder, name), content);
        }
        return body(folder);
    }
    finally
    {
        rmSync(folder, { recursive: true, force: true });
    }
}

const crate = (name, version, license, ...files) => ({
    name,
    version,
    license,
    repository: `https://example.com/${name}`,
    files: files.map(([kind, text]) => ({ name: `${kind.toUpperCase()}-${text.length}`, kind, text })),
});

test('an OR is met by any one of its licenses', () => {
    assert.equal(acceptsLicense('MIT OR GPL-3.0-only', ACCEPTED), true);
    assert.equal(acceptsLicense('GPL-2.0-only OR GPL-3.0-only', ACCEPTED), false);
});

test('an AND is met only by all of its licenses', () => {
    assert.equal(acceptsLicense('MIT AND Unicode-3.0', ACCEPTED), true);
    assert.equal(acceptsLicense('MIT AND GPL-3.0-only', ACCEPTED), false);
});

test('parentheses group an expression', () => {
    assert.equal(acceptsLicense('(MIT OR Apache-2.0) AND Unicode-3.0', ACCEPTED), true);
    assert.equal(acceptsLicense('(GPL-3.0-only OR LGPL-3.0-only) AND MIT', ACCEPTED), false);
    assert.equal(acceptsLicense('MIT OR (GPL-3.0-only AND Apache-2.0)', ACCEPTED), true);
});

test('the old slash of Cargo.toml means OR', () => {
    assert.equal(acceptsLicense('MIT/Apache-2.0', ACCEPTED), true);
    assert.equal(acceptsLicense('BSD-3-Clause/GPL-3.0-only', ACCEPTED), true);
    assert.equal(acceptsLicense('GPL-3.0-only/LGPL-3.0-only', ACCEPTED), false);
});

test('an exception is met by the license it adds to', () => {
    assert.equal(acceptsLicense('Apache-2.0 WITH LLVM-exception', ACCEPTED), true);
    assert.equal(acceptsLicense('GPL-2.0-only WITH Classpath-exception-2.0', ACCEPTED), false);
});

test('an expression that does not parse is an error naming it', () => {
    for (const expression of ['MIT AND', '(MIT OR Apache-2.0', 'MIT Apache-2.0', ''])
    {
        assert.throws(() => acceptsLicense(expression, ACCEPTED), (error) => error instanceof LicenseError
            && error.message.includes(`"${expression}"`));
    }
});

const TREE = [
    '0|lpdf v0.0.0 (/work/src/core)',
    '1|base64 v0.22.1',
    '1|serde v1.0.228',
    '2|serde_derive v1.0.228 (proc-macro)',
    '3|proc-macro2 v1.0.106',
    '4|unicode-ident v1.0.18',
    '3|quote v1.0.45',
    '4|proc-macro2 v1.0.106',
    '3|syn v2.0.117',
    '1|pdf-writer v0.12.1',
    '2|itoa v1.0.15',
    '1|unicode-ident v1.0.18',
    '1|tiff v0.11.0',
    '2|proc-macro2 v1.0.106',
    '',
].join('\n');

test('a build links the crates of its tree', () => {
    const linked = linkedCrates(TREE);
    for (const crateName of ['lpdf 0.0.0', 'base64 0.22.1', 'serde 1.0.228', 'pdf-writer 0.12.1', 'itoa 1.0.15'])
    {
        assert.ok(linked.includes(crateName), `${crateName} is linked`);
    }
});

test('a crate whose name starts with a digit is read whole', () => {
    const linked = linkedCrates(['0|lpdf v0.0.0 (/work/src/core)', '1|27z v0.1.0', '2|7zip-sys v1.2.3', ''].join('\n'));
    assert.deepEqual(linked, ['lpdf 0.0.0', '27z 0.1.0', '7zip-sys 1.2.3']);
});

test('a proc-macro crate and what only it needs are not linked', () => {
    const linked = linkedCrates(TREE);
    for (const crateName of ['serde_derive 1.0.228', 'quote 1.0.45', 'syn 2.0.117'])
    {
        assert.ok(!linked.includes(crateName), `${crateName} is not linked`);
    }
});

test('a crate that a proc-macro crate shares with a linked crate stays', () => {
    const linked = linkedCrates(TREE);
    assert.ok(linked.includes('unicode-ident 1.0.18'));
    assert.ok(linked.includes('proc-macro2 1.0.106'));
});

test('a text is read the same on every system', () => {
    assert.equal(normalizeText('\uFEFFMIT License  \r\n\r\nCopyright (c) 2016 A\t\r\n\r\n'), 'MIT License\n\nCopyright (c) 2016 A');
    assert.equal(normalizeText('a\rb'), 'a\nb');
});

test('the license and NOTICE files of a folder are read, and nothing else', () => {
    const files = {
        'LICENSE-MIT': 'mit text',
        'LICENSE-APACHE': 'apache text',
        'NOTICE': 'notice text',
        'COPYING': 'copying text',
        'UNLICENSE': 'unlicense text',
        'license.md': 'markdown text',
        'README.md': 'readme',
        'LICENSES/MIT.txt': 'a folder is no file of the crate',
        'src/lib.rs': 'code',
    };
    withFolder(files, (folder) => {
        const read = licenseFiles(folder);
        assert.deepEqual(read.map((file) => file.name), ['COPYING', 'LICENSE-APACHE', 'LICENSE-MIT', 'NOTICE', 'UNLICENSE', 'license.md']);
        assert.deepEqual(read.map((file) => file.kind), ['license', 'license', 'license', 'notice', 'license', 'license']);
    });
});

/** The package of a crate on disk, as `cargo metadata` lists it. */
function packageIn(folder, name, version, license)
{
    return { name, version, license, repository: null, manifest_path: path.join(folder, name, 'Cargo.toml') };
}

test('a crate with an accepted license and a text is collected without a problem', () => {
    withFolder({ 'one/Cargo.toml': '', 'one/LICENSE-MIT': 'mit text' }, (folder) => {
        const { crates, problems } = collectCrates([packageIn(folder, 'one', '1.0.0', 'MIT OR GPL-3.0-only')], {
            accepted: ACCEPTED, overrides: {}, overridesDir: folder,
        });
        assert.deepEqual(problems, []);
        assert.equal(crates[0].files[0].text, 'mit text');
    });
});

test('a license that none of the accepted licenses meets is a problem naming the crate', () => {
    withFolder({ 'one/LICENSE': 'text' }, (folder) => {
        const { problems } = collectCrates([packageIn(folder, 'one', '1.2.3', 'GPL-3.0-only')], {
            accepted: ACCEPTED, overrides: {}, overridesDir: folder,
        });
        assert.equal(problems.length, 1);
        assert.match(problems[0], /^one 1\.2\.3 is licensed "GPL-3\.0-only"/);
    });
});

test('a crate with no license expression is a problem', () => {
    withFolder({ 'one/LICENSE': 'text' }, (folder) => {
        const { problems } = collectCrates([packageIn(folder, 'one', '1.0.0', null)], {
            accepted: ACCEPTED, overrides: {}, overridesDir: folder,
        });
        assert.match(problems.join('\n'), /one 1\.0\.0 names no license expression/);
    });
});

test('a crate with no license file is a problem', () => {
    withFolder({ 'one/Cargo.toml': '' }, (folder) => {
        const { problems } = collectCrates([packageIn(folder, 'one', '1.0.0', 'MIT')], {
            accepted: ACCEPTED, overrides: {}, overridesDir: folder,
        });
        assert.match(problems.join('\n'), /one 1\.0\.0 has no license file/);
    });
});

test('an override gives the text of a crate that has no license file', () => {
    withFolder({ 'one/Cargo.toml': '', 'texts/one.LICENSE': 'written text\r\n' }, (folder) => {
        const { crates, problems } = collectCrates([packageIn(folder, 'one', '1.0.0', 'BSD-3-Clause')], {
            accepted: ACCEPTED, overrides: { 'one 1.0.0': 'one.LICENSE' }, overridesDir: path.join(folder, 'texts'),
        });
        assert.deepEqual(problems, []);
        assert.equal(crates[0].files[0].text, 'written text');
    });
});

test('an override of another version of the crate is a problem, and the crate has none', () => {
    withFolder({ 'one/Cargo.toml': '', 'texts/one.LICENSE': 'text' }, (folder) => {
        const { problems } = collectCrates([packageIn(folder, 'one', '1.1.0', 'MIT')], {
            accepted: ACCEPTED, overrides: { 'one 1.0.0': 'one.LICENSE' }, overridesDir: path.join(folder, 'texts'),
        });
        assert.equal(problems.length, 2);
        assert.match(problems[0], /one 1\.1\.0 has no license file/);
        assert.match(problems[1], /OVERRIDES has an entry for one 1\.0\.0/);
    });
});

test('an override of a crate that has a license file of its own is a problem', () => {
    withFolder({ 'one/LICENSE': 'its own', 'texts/one.LICENSE': 'text' }, (folder) => {
        const { problems } = collectCrates([packageIn(folder, 'one', '1.0.0', 'MIT')], {
            accepted: ACCEPTED, overrides: { 'one 1.0.0': 'one.LICENSE' }, overridesDir: path.join(folder, 'texts'),
        });
        assert.match(problems.join('\n'), /OVERRIDES has an entry for one 1\.0\.0/);
    });
});

test('the same text is written once, with every crate that carries it', () => {
    const output = render([
        crate('b', '2.0.0', 'MIT', ['license', 'shared text']),
        crate('a', '1.0.0', 'MIT', ['license', 'shared text']),
        crate('c', '1.0.0', 'MIT', ['license', 'another text']),
    ]);
    assert.equal(output.split('shared text').length - 1, 1);
    assert.match(output, /License text of:\n {4}a 1\.0\.0, b 2\.0\.0\n-+\nshared text\n/);
    assert.match(output, /License text of:\n {4}c 1\.0\.0\n-+\nanother text\n/);
    assert.match(output, /LICENSE TEXTS \(2\)/);
});

test('a crate with two license files is under both texts', () => {
    const output = render([crate('a', '1.0.0', 'MIT OR Apache-2.0', ['license', 'mit text'], ['license', 'apache text'])]);
    assert.match(output, /License text of:\n {4}a 1\.0\.0\n-+\nmit text\n/);
    assert.match(output, /License text of:\n {4}a 1\.0\.0\n-+\napache text\n/);
});

test('a NOTICE is kept apart from a license of the same words', () => {
    const output = render([crate('a', '1.0.0', 'Apache-2.0', ['license', 'same words'], ['notice', 'same words'])]);
    assert.match(output, /License text of:\n {4}a 1\.0\.0\n-+\nsame words\n/);
    assert.match(output, /NOTICE of:\n {4}a 1\.0\.0\n-+\nsame words\n/);
});

test('the crates are listed with their license and repository, by name and version', () => {
    const output = render([
        crate('b', '1.0.0', 'MIT', ['license', 'x']),
        crate('a', '2.0.0', 'Apache-2.0', ['license', 'x']),
        crate('a', '10.0.0', 'MIT', ['license', 'x']),
    ]);
    const list = output.split('\n').filter((line) => line.includes(' | '));
    assert.deepEqual(list, [
        'a 10.0.0 | MIT | https://example.com/a',
        'a 2.0.0 | Apache-2.0 | https://example.com/a',
        'b 1.0.0 | MIT | https://example.com/b',
    ]);
});

test('the file is the same whatever the order of the crates', () => {
    const crates = [
        crate('c', '1.0.0', 'MIT', ['license', 'one']),
        crate('a', '1.0.0', 'MIT', ['license', 'two']),
        crate('b', '1.0.0', 'MIT', ['license', 'one'], ['license', 'two']),
    ];
    assert.equal(render(crates), render([...crates].reverse()));
});

test('a long list of crates is broken into lines of at most 80 characters', () => {
    const crates = Array.from({ length: 30 }, (_, index) => crate(`crate-${String(index).padStart(2, '0')}`, '1.0.0', 'MIT', ['license', 'x']));
    const output = render(crates);
    for (const line of output.split('\n').filter((text) => text.startsWith('    crate-')))
    {
        assert.ok(line.length <= 80, `"${line}" is ${line.length} characters`);
    }
    assert.ok(output.endsWith('x\n'));
});

/** A part of the toolchain, with the names of its text files. */
const part = (name, license, files, extra = {}) => ({ name, license, repository: `https://example.com/${name}`, files, ...extra });

test('a part of the toolchain is named with its version and where it comes from', () => {
    assert.equal(toolchainLabel({ name: 'wasi-libc' }), 'wasi-libc');
    assert.equal(toolchainLabel({ name: 'hashbrown', version: '0.17.1', origin: 'Rust standard library' }),
        'hashbrown 0.17.1 (Rust standard library)');
    assert.equal(toolchainLabel({ name: 'musl', origin: 'wasi-libc' }), 'musl (wasi-libc)');
});

test('the texts of a part of the toolchain are read from its folder', () => {
    withFolder({ 'std-MIT.txt': 'mit text\r\n', 'std-APACHE.txt': 'apache text' }, (folder) => {
        const { parts, problems } = collectToolchain([part('std', 'MIT OR Apache-2.0', ['std-MIT.txt', 'std-APACHE.txt'])], {
            accepted: ACCEPTED, directory: folder,
        });
        assert.deepEqual(problems, []);
        assert.deepEqual(parts[0].files.map((file) => file.text), ['mit text', 'apache text']);
    });
});

test('a part of the toolchain under a license that none of the accepted licenses meets is a problem', () => {
    withFolder({ 'std.txt': 'text' }, (folder) => {
        const { problems } = collectToolchain([part('std', 'GPL-3.0-only', ['std.txt'])], { accepted: ACCEPTED, directory: folder });
        assert.equal(problems.length, 1);
        assert.match(problems[0], /^std is licensed "GPL-3\.0-only"/);
    });
});

test('a text that a part names and the folder lacks is a problem naming both', () => {
    withFolder({ 'std.txt': 'text' }, (folder) => {
        const { problems } = collectToolchain([part('std', 'MIT', ['std.txt', 'gone.txt'])], { accepted: ACCEPTED, directory: folder });
        assert.deepEqual(problems, ['std names gone.txt, which third-party-licenses/toolchain/ doesn\'t have']);
    });
});

test('a text in the folder that no part names is a problem', () => {
    withFolder({ 'std.txt': 'text', 'left-over.txt': 'text' }, (folder) => {
        const { problems } = collectToolchain([part('std', 'MIT', ['std.txt'])], { accepted: ACCEPTED, directory: folder });
        assert.deepEqual(problems, ['third-party-licenses/toolchain/left-over.txt is named by no part of TOOLCHAIN: remove it']);
    });
});

test('the real list of toolchain parts reads with no problem', () => {
    const { parts, problems } = collectToolchain(TOOLCHAIN);
    assert.deepEqual(problems, []);
    for (const each of parts) assert.ok(each.files.length > 0, `${toolchainLabel(each)} has a text`);
});

test('the parts of the toolchain are listed after the crates, with where a build uses them', () => {
    const output = render(
        [crate('a', '1.0.0', 'MIT', ['license', 'one'])],
        [{ ...part('wasi-libc', 'MIT', []), usedIn: 'the WASI build', files: [{ name: 'x', kind: 'license', text: 'two' }] }],
    );
    assert.match(output, /\nCRATES \(1\)\n/);
    assert.match(output, /\nRUST TOOLCHAIN \(1\)\n=+\n\nwasi-libc \| MIT \| https:\/\/example\.com\/wasi-libc \| in the WASI build\n/);
    assert.ok(output.indexOf('CRATES (') < output.indexOf('RUST TOOLCHAIN ('));
    assert.ok(output.indexOf('RUST TOOLCHAIN (') < output.indexOf('LICENSE TEXTS ('));
});

test('a text that a crate and a part of the toolchain share is written once, with both', () => {
    const shared = { name: 'x', kind: 'license', text: 'shared text' };
    const output = render([crate('a', '1.0.0', 'MIT', ['license', 'shared text'])], [{ ...part('std', 'MIT', []), files: [shared] }]);
    assert.equal(output.split('shared text').length - 1, 1);
    assert.match(output, /License text of:\n {4}a 1\.0\.0, std\n-+\nshared text\n/);
});

test('a file with no parts of the toolchain has no section for them', () => {
    assert.doesNotMatch(render([crate('a', '1.0.0', 'MIT', ['license', 'one'])]), /RUST TOOLCHAIN/);
});

test('the texts of a schema are read from its folder, a NOTICE as a notice', () => {
    withFolder({ 'vendor/a/LICENSE': 'apache text', 'vendor/a/NOTICE': 'the notice' }, (folder) => {
        const schema = { name: 'A schema', license: 'Apache-2.0', folder: 'vendor/a', files: ['LICENSE', 'NOTICE'] };
        const { schemas, problems } = collectSchemas([schema], { accepted: ACCEPTED, root: folder });
        assert.deepEqual(problems, []);
        assert.deepEqual(schemas[0].files.map((file) => [file.kind, file.text]), [['license', 'apache text'], ['notice', 'the notice']]);
    });
});

test('a schema under a license none of the accepted meets, or without its text, is a problem', () => {
    withFolder({ 'vendor/a/LICENSE': 'text' }, (folder) => {
        const schema = { name: 'A schema', license: 'GPL-3.0-only', folder: 'vendor/a', files: ['LICENSE', 'NOTICE'] };
        const { problems } = collectSchemas([schema], { accepted: ACCEPTED, root: folder });
        assert.equal(problems.length, 2);
        assert.match(problems[0], /^A schema is licensed "GPL-3.0-only"/);
        assert.equal(problems[1], 'A schema names vendor/a/NOTICE, which isn\'t there');
    });
});

test('the real list of schemas reads with no problem', () => {
    const { schemas, problems } = collectSchemas(SCHEMAS);
    assert.deepEqual(problems, []);
    for (const each of schemas) assert.equal(each.files.length, 2, `${each.name} has its LICENSE and NOTICE`);
});

test('the schemas are listed last, and their texts among the others', () => {
    const schema = { name: 'A schema', version: '1.0', license: 'Apache-2.0', repository: 'https://example.com/a',
        files: [{ name: 'NOTICE', kind: 'notice', text: 'the notice' }] };
    const output = render([crate('a', '1.0.0', 'MIT', ['license', 'one'])], [], [schema]);
    assert.match(output, /\nSCHEMAS \(1\)\n=+\n\nA schema 1\.0 \| Apache-2\.0 \| https:\/\/example\.com\/a\n/);
    assert.ok(output.indexOf('CRATES (') < output.indexOf('SCHEMAS ('));
    assert.ok(output.indexOf('SCHEMAS (') < output.indexOf('LICENSE TEXTS ('));
    assert.match(output, /NOTICE of:\n    A schema 1\.0\n-+\nthe notice\n/);
    assert.doesNotMatch(render([crate('a', '1.0.0', 'MIT', ['license', 'one'])]), /SCHEMAS/);
});
