// Writes the API that follows from lpdf.xsd into the four SDKs: the attribute classes, and the constants for
// the schema's enumerations. Every SDK gets the same classes and constants with the schema's names,
// differing only in the case rule of its language: font-size is fontSize in Node and PHP, font_size in
// Python and FontSize in .NET.
//
// A schema attribute marked use="required" is required in every SDK, by each language's own means: a
// required property in TypeScript, a required `init` property in C#, a field without a default in Python
// and a constructor parameter without a default in PHP. Data-binding attributes (data-*) are for XML
// templates and are left out.
//
// The constants are strings, so a value goes straight into an attribute: type: FieldType.Text in Node,
// FieldAttr(type=FieldType.TEXT) in Python.
//
// Usage: node scripts/gen-sdk-api.mjs                      write the files of the four SDKs under src/sdk
//        node scripts/gen-sdk-api.mjs --check              list the files that differ, exit 1 if any do
//        node scripts/gen-sdk-api.mjs --check --sdk node=.  only the SDK in the given folder (an SDK repository's CI)

import { mkdirSync, readFileSync, writeFileSync, existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { sdkRepositories } from './lib/sdk-repositories.mjs';
import { loadSchema } from './lib/xsd.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const schema = loadSchema(path.join(root, 'schema/lpdf.xsd'));
const check = process.argv.includes('--check');
const { repositories, selected } = sdkRepositories(root, process.argv);

const SDK = {
    dotnet: path.join(repositories.dotnet, 'src'),
    node: path.join(repositories.node, 'src'),
    python: path.join(repositories.python, 'src/lpdf'),
    php: path.join(repositories.php, 'src'),
};

// `where` is how the schema declares the element (see loadSchema); `area` is the folder it lives in.
const CLASSES = [
    ...['stack', 'flank', 'split', 'cluster', 'grid', 'frame', 'link', 'divider', 'span', 'img', 'barcode',
        'text', 'td', 'thead', 'tr', 'table', 'field'].map((el) => ({ cls: `${pascal(el)}Attr`, el, where: 'global', area: 'layout' })),
    { cls: 'RegionAttr', el: 'region', where: 'layout', area: 'layout' },
    { cls: 'LayerAttr', el: 'layer', where: 'canvas', area: 'canvas' },
    ...['rect', 'circle', 'ellipse', 'line', 'path'].map((el) => ({ cls: `${pascal(el)}Attr`, el, where: 'primitives', area: 'canvas' })),
    { cls: 'CanvasTextAttr', el: 'text', where: 'primitives', area: 'canvas' },
    { cls: 'CanvasImgAttr', el: 'img', where: 'primitives', area: 'canvas' },
    { cls: 'FontAttr', el: 'font', where: 'global', area: 'kit' },
    { cls: 'ImageAttr', el: 'image', where: 'global', area: 'kit' },
];

// Where the classes of an area live in each SDK.
const PLACES = {
    layout: { dotnet: 'Lpdf.Layout', php: 'Lpdf\\Layout', folder: 'Layout', python: 'layout' },
    canvas: { dotnet: 'Lpdf.Canvas', php: 'Lpdf\\Canvas', folder: 'Canvas', python: 'canvas' },
    kit: { dotnet: 'Lpdf.Kit', php: 'Lpdf\\Kit', folder: 'Kit', python: 'kit' },
};

function words(name) { return name.split('-'); }
function pascal(name) { return words(name).map((w) => w[0].toUpperCase() + w.slice(1)).join(''); }
function camel(name) { const p = pascal(name); return p[0].toLowerCase() + p.slice(1); }
function snake(name) { return words(name).join('_'); }
function snakeFromPascal(name) { return name.replace(/([a-z0-9])([A-Z])/g, '$1_$2').toLowerCase(); }

/** Wraps `text` to `width` columns, each line starting with `prefix`. */
function wrap(text, width, prefix) {
    const lines = [];
    let line = '';
    for (const word of text.split(' ')) {
        if (line && (prefix + line + ' ' + word).length > width) { lines.push(prefix + line); line = word; }
        else line = line ? `${line} ${word}` : word;
    }
    if (line) lines.push(prefix + line);
    return lines;
}

function xmlDoc(text) {
    return text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/`([^`]+)`/g, '<c>$1</c>');
}

function summary(spec, element) {
    if (element.doc) return element.doc;
    return `Attributes of the \`${spec.el}\` element${spec.area === 'canvas' && spec.where === 'primitives' ? ' on the canvas' : ''}.`;
}

/** Required attributes first, in schema order, then the optional ones in schema order. */
function ordered(element) {
    const attributes = element.attributes.filter((a) => !a.name.startsWith('data-'));
    return [...attributes.filter((a) => a.required), ...attributes.filter((a) => !a.required)];
}

const HEADER = [
    'Generated from lpdf.xsd by scripts/gen-sdk-api.mjs.',
    'Do not edit: change the schema and run `make gen-sdk-api`.',
];

// ── .NET ──────────────────────────────────────────────────────────────────────

function dotnet(spec, element) {
    const lines = [...HEADER.map((h) => `// ${h}`), `namespace ${PLACES[spec.area].dotnet};`, '', '#pragma warning disable CS1591',
        `/// <summary>${xmlDoc(summary(spec, element))}</summary>`, `public sealed record ${spec.cls}`, '{'];
    ordered(element).forEach((a, i) => {
        if (a.doc) lines.push(...(i > 0 ? [''] : []), `    /// <summary>${xmlDoc(a.doc)}</summary>`);
        lines.push(a.required
            ? `    public required string ${pascal(a.name)} { get; init; }`
            : `    public string? ${pascal(a.name)} { get; init; }`);
    });
    lines.push('}', '');
    return { file: path.join(SDK.dotnet, PLACES[spec.area].folder, `${spec.cls}.cs`), text: lines.join('\n') };
}

// ── PHP ───────────────────────────────────────────────────────────────────────

function php(spec, element) {
    const lines = ['<?php', '', 'declare(strict_types=1);', '', `namespace ${PLACES[spec.area].php};`, '', '/**',
        ...wrap(summary(spec, element), 100, ' * '), ' *', ...HEADER.map((h) => ` * ${h}`), ' */',
        `final readonly class ${spec.cls}`, '{', '    public function __construct('];
    for (const a of ordered(element)) {
        if (a.doc) lines.push(...(a.doc.length > 80 ? ['        /**', ...wrap(a.doc, 100, '         * '), '         */'] : [`        /** ${a.doc} */`]));
        lines.push(a.required ? `        public string $${camel(a.name)},` : `        public ?string $${camel(a.name)} = null,`);
    }
    lines.push('    ) {}', '}', '');
    return { file: path.join(SDK.php, PLACES[spec.area].folder, `${spec.cls}.php`), text: lines.join('\n') };
}

// ── Python ────────────────────────────────────────────────────────────────────

function python(spec, element) {
    const lines = [...HEADER.map((h) => `# ${h}`), 'from __future__ import annotations', '', 'from dataclasses import dataclass', '', '',
        '@dataclass(frozen=True)', `class ${spec.cls}:`];
    const doc = wrap(summary(spec, element), 92, '    ');
    lines.push(doc.length === 1 ? `    """${doc[0].trim()}"""` : ['    """', ...doc, '    """'].join('\n'), '');
    for (const a of ordered(element)) {
        if (a.doc) lines.push(...wrap(a.doc, 96, '    # '));
        lines.push(a.required ? `    ${snake(a.name)}: str` : `    ${snake(a.name)}: str | None = None`);
    }
    lines.push('');
    return { file: path.join(SDK.python, PLACES[spec.area].python, `${snakeFromPascal(spec.cls)}.py`), text: lines.join('\n') };
}

// ── Node ──────────────────────────────────────────────────────────────────────

function nodeInterface(spec, element) {
    const lines = ['/**', ...wrap(summary(spec, element), 100, ' * '), ' */', `export interface ${spec.cls} {`];
    for (const a of ordered(element)) {
        if (a.doc) lines.push(...(a.doc.length > 80 ? ['  /**', ...wrap(a.doc, 100, '   * '), '   */'] : [`  /** ${a.doc} */`]));
        lines.push(`  ${camel(a.name)}${a.required ? '' : '?'}: string;`);
    }
    lines.push('}', '');
    return lines.join('\n');
}

// ── Constants ─────────────────────────────────────────────────────────────────

// The schema's enumerations that the SDKs name. `area` is the folder of the .NET and PHP file and the
// Python subpackage. A constant's name is its value, in each language's case.
const CONSTANTS = [
    { name: 'FieldType', area: 'layout', file: 'field_type', values: () => schema.attributeValues('field', 'type'),
      doc: 'The values of the `type` attribute of a field.' },
    { name: 'Pin', area: 'layout', file: 'pin', values: () => schema.simpleTypeValues('RegionPin'),
      doc: 'The values of the `pin` attribute of a region.' },
    { name: 'Orientation', area: 'kit', file: 'orientation', values: () => schema.simpleTypeValues('Orientation'),
      doc: 'The values of the `orientation` attribute of a document or a section.' },
    { name: 'BuiltinFont', area: 'kit', file: 'builtin_font', values: () => schema.simpleTypeValues('BuiltinFont'),
      doc: 'The values of the `core` attribute of a font: the built-in PDF fonts, which need no file.' },
    { name: 'PageScope', area: 'shared', file: 'page_scope', values: () => schema.simpleTypeValues('PageScope'),
      doc: 'The named values of the `page` attribute of a layer or a region. A range such as 2-4 or 1,3-5 is a string.' },
];

const AREAS = {
    layout: { folder: 'Layout', dotnet: 'Lpdf.Layout', php: 'Lpdf\\Layout' },
    kit: { folder: 'Kit', dotnet: 'Lpdf.Kit', php: 'Lpdf\\Kit' },
    shared: { folder: 'Shared', dotnet: 'Lpdf.Shared', php: 'Lpdf\\Shared' },
};

function upperSnake(value) { return value.replace(/-/g, '_').toUpperCase(); }

function constantsFor(spec) {
    const values = spec.values();
    const area = AREAS[spec.area];

    const dotnetLines = [
        ...HEADER.map((h) => `// ${h}`), `namespace ${area.dotnet};`, '', '#pragma warning disable CS1591',
        `/// <summary>${xmlDoc(spec.doc)}</summary>`, `public static class ${spec.name}`, '{',
        ...values.map((v) => `    public const string ${pascal(v)} = "${v}";`), '}', '',
    ];

    const phpLines = [
        '<?php', '', 'declare(strict_types=1);', '', `namespace ${area.php};`, '', '/**',
        ...wrap(spec.doc, 100, ' * '), ' *', ...HEADER.map((h) => ` * ${h}`), ' */', `final class ${spec.name}`, '{',
        ...values.map((v) => `    public const string ${pascal(v)} = '${v}';`), '}', '',
    ];

    const pythonLines = [
        ...HEADER.map((h) => `# ${h}`), 'from enum import StrEnum', '', '',
        `class ${spec.name}(StrEnum):`, `    """${spec.doc}"""`, '',
        ...values.map((v) => `    ${upperSnake(v)} = "${v}"`), '',
    ];

    const nodeLines = [
        `/** ${spec.doc} */`, `export const ${spec.name} = {`, ...values.map((v) => `  ${pascal(v)}: '${v}',`), '} as const;', '',
        `export type ${spec.name} = (typeof ${spec.name})[keyof typeof ${spec.name}];`, '',
    ];

    return {
        files: [
            { file: path.join(SDK.dotnet, area.folder, `${spec.name}.cs`), text: dotnetLines.join('\n') },
            { file: path.join(SDK.php, area.folder, `${spec.name}.php`), text: phpLines.join('\n') },
            { file: path.join(SDK.python, spec.area, `${spec.file}.py`), text: pythonLines.join('\n') },
        ],
        node: nodeLines.join('\n'),
    };
}

// ── Run ───────────────────────────────────────────────────────────────────────

const generated = [];
const nodeInterfaces = [];
for (const spec of CLASSES) {
    const element = schema.element(spec.el, spec.where);
    generated.push(dotnet(spec, element), php(spec, element), python(spec, element));
    nodeInterfaces.push(nodeInterface(spec, element));
}
const nodeConstants = [];
for (const spec of CONSTANTS) {
    const made = constantsFor(spec);
    generated.push(...made.files);
    nodeConstants.push(made.node);
}
generated.push({
    file: path.join(SDK.node, 'constants.ts'),
    text: [...HEADER.map((h) => `// ${h}`), '', ...nodeConstants].join('\n'),
});
generated.push({
    file: path.join(SDK.node, 'attrs.ts'),
    text: [...HEADER.map((h) => `// ${h}`), '', ...nodeInterfaces].join('\n'),
});

// Only the SDKs asked for: the files of the others are not there to compare with.
const inSelected = (file) => selected.some((language) => file.startsWith(SDK[language] + path.sep));
const outputs = generated.filter(({ file }) => inSelected(file));

const normal = (s) => s.replace(/\r\n/g, '\n');
let differing = 0;
for (const { file, text } of outputs) {
    const current = existsSync(file) ? normal(readFileSync(file, 'utf8')) : null;
    if (current === text) continue;
    differing++;
    if (check) {
        console.log(`${current === null ? 'missing' : 'differs'}: ${path.relative(process.cwd(), file)}`);
    } else {
        mkdirSync(path.dirname(file), { recursive: true });
        writeFileSync(file, text);
        console.log(`wrote ${path.relative(process.cwd(), file)}`);
    }
}
console.log(check ? `${differing} of ${outputs.length} generated files differ` : `${differing} of ${outputs.length} files written`);
if (check && differing) process.exit(1);
