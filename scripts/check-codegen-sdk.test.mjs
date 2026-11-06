// Tests of the parts of scripts/check-codegen-sdk.mjs that need no SDK: how each language's program is taken apart, which
// assets a document declares, and how a program's results are read back.
//
//   node --test scripts/check-codegen-sdk.test.mjs

import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { test } from 'node:test';

import { declaredAssets } from './lib/codegen-check/assets.mjs';
import { readResults } from './lib/codegen-check/common.mjs';
import * as dotnet from './lib/codegen-check/dotnet.mjs';
import * as java from './lib/codegen-check/java.mjs';
import * as node from './lib/codegen-check/node.mjs';
import * as php from './lib/codegen-check/php.mjs';
import * as python from './lib/codegen-check/python.mjs';

// What the code generator writes for `<lpdf version="1"><document><section><layout><text>Hi</text></layout></section></document></lpdf>`.
const JS = "import { L, NoAttr } from '@lpdfio/lpdf'\n\nconst engine = L.engine()\n\nconst doc = L.document(NoAttr, [\n    L.section(NoAttr, [\n        L.layout(NoAttr, [\n            L.text(NoAttr, ['Hi']),\n        ]),\n    ]),\n])\n\nconst pdf = await engine.render(doc)\n";
const DOTNET = 'using Lpdf;\nusing static Lpdf.L;\n\nvar engine = L.Engine();\n\nvar doc = L.Document(NoAttr, [\n    L.Section(NoAttr, [\n        L.Layout(NoAttr, [\n            L.Text(NoAttr, ["Hi"]),\n        ]),\n    ]),\n]);\n\nvar pdf = engine.Render(doc);\n';
const PHP = "<?php\n\nrequire_once 'vendor/autoload.php';\n\nuse Lpdf\\L;\nuse const Lpdf\\NoAttr;\n\n$engine = L::engine();\n\n$doc = L::document(NoAttr, [\n    L::section(NoAttr, [\n        L::layout(NoAttr, [\n            L::text(NoAttr, ['Hi']),\n        ]),\n    ]),\n]);\n\n$pdf = $engine->render($doc);\n";
const PYTHON = "from lpdf import L, NoAttr\n\nengine = L.engine()\n\ndoc = L.document(NoAttr, [\n    L.section(NoAttr, [\n        L.layout(NoAttr, [\n            L.text(NoAttr, ['Hi'])\n        ])\n    ])\n])\n\npdf = engine.render(doc)\n";
const JAVA = 'import io.lpdf.*;\nimport java.util.List;\nimport static io.lpdf.L.NoAttr;\n\npublic class Main {\n    public static void main(String[] args) {\n        var engine = L.engine();\n\n        var doc = L.document(NoAttr, List.of(\n            L.section(NoAttr, List.of(\n                L.layout(NoAttr, List.of(\n                    L.text(NoAttr, List.of("Hi"))\n                ))\n            ))\n        ));\n\n        byte[] pdf = engine.render(doc);\n    }\n}\n';

test('JS: the imported names and the expression of the document', () => {
    const { names, expression } = node.shape(JS);
    assert.deepEqual(names, ['L', 'NoAttr']);
    assert.ok(expression.startsWith('L.document(NoAttr, ['));
    assert.ok(expression.endsWith('])'));
    assert.ok(!expression.includes('engine'));
});

test('C#: a class whose Build returns the document, without the engine and the render', () => {
    const source = dotnet.shape(7, DOTNET);
    assert.match(source, /^using Lpdf;\nusing static Lpdf\.L;\n/);
    assert.match(source, /public static class Doc7\n\{\n {4}public static Lpdf\.Kit\.PdfDocument Build\(\)\n {4}\{\nvar doc = L\.Document\(/);
    assert.match(source, /\n {8}return doc;\n/);
    assert.ok(!source.includes('engine'));
});

test('PHP: a file that returns the document, without the autoloader and the render', () => {
    const source = php.shape(PHP);
    assert.ok(source.startsWith('<?php\n\nuse Lpdf\\L;\nuse const Lpdf\\NoAttr;\n'));
    assert.ok(source.endsWith('\n\nreturn $doc;\n'));
    assert.ok(!source.includes('autoload') && !source.includes('->render('));
});

test('Python: a module whose doc is the document, without the render', () => {
    const source = python.shape(PYTHON);
    assert.ok(source.startsWith('from lpdf import L, NoAttr\n'));
    assert.ok(source.includes('\ndoc = L.document('));
    assert.ok(!source.includes('.render('));
});

test('Java: a class of its own whose build returns the document, without the engine and the render', () => {
    const source = java.shape(7, JAVA);
    assert.ok(source.includes('\npublic class Doc7 {\n'));
    assert.ok(source.includes('    public static PdfDocument build() {\n        var doc = L.document('));
    assert.ok(source.includes('\n\n        return doc;\n    }\n}\n'));
    assert.ok(!source.includes('engine') && !source.includes('main('));
});

test('a program in another shape is refused, naming what the check expects', () => {
    assert.throws(() => node.shape('const doc = 1'), /import/);
    assert.throws(() => dotnet.shape(1, 'var doc = 1;'), /using/);
    assert.throws(() => php.shape('<?php $doc = 1;'), /autoloader/);
    assert.throws(() => python.shape('doc = 1'), /import/);
    assert.throws(() => java.shape(1, 'class A {}'), /class Main/);
});

test('the assets a document declares are loaded under their ref, else their name', () => {
    const folder = mkdtempSync(path.join(tmpdir(), 'lpdf-assets-test-'));
    try {
        mkdirSync(path.join(folder, 'fonts'));
        writeFileSync(path.join(folder, 'fonts', 'real.ttf'), 'font');
        const placeholders = { font: 'font.ttf', image: 'image.png', file: 'file.txt', profile: 'profile.icc', 'factur-x.xml': 'invoice.xml' };
        const xml = [
            '<assets><font name="body" src="fonts/real.ttf"/><font name="other" ref="custom" src="missing.ttf"/>',
            '<image name="logo"/><image data-ref="no" name="seal" ref="stamp"/><profile name="press"/></assets>',
            '<attachments><file name="factur-x.xml"/><file name="terms.txt" ref="terms"/></attachments>',
        ].join('');
        assert.deepEqual(declaredAssets(xml, folder, placeholders), [
            { kind: 'font', key: 'body', path: path.join(folder, 'fonts', 'real.ttf') },
            { kind: 'font', key: 'custom', path: 'font.ttf' },
            { kind: 'image', key: 'logo', path: 'image.png' },
            { kind: 'image', key: 'stamp', path: 'image.png' },
            { kind: 'file', key: 'factur-x.xml', path: 'invoice.xml' },
            { kind: 'file', key: 'terms', path: 'file.txt' },
            { kind: 'profile', key: 'press', path: 'profile.icc' },
        ]);
    } finally {
        rmSync(folder, { recursive: true, force: true });
    }
});

test('the results a program printed are read back by index', () => {
    const results = readResults('noise\nRESULT\t3\tok\t\nRESULT\t4\tdiffers\t\nRESULT\t5\tcode\tNameError: x\ty\n');
    assert.deepEqual([...results], [
        [3, { status: 'ok', message: '' }],
        [4, { status: 'differs', message: '' }],
        [5, { status: 'code', message: 'NameError: x y' }],
    ]);
});
