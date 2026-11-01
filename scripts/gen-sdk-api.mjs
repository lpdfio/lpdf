// Writes the API that follows from lpdf.xsd into the four SDKs: the attribute classes, and the constants for
// the schema's enumerations. Every SDK gets the same classes and constants with the schema's names,
// differing only in the case rule of its language: font-size is fontSize in Node and PHP, font_size in
// Python and FontSize in .NET.
//
// Each attribute has the type the schema gives it: a boolean is `boolean`, `bool` or `bool`, a whole number `number`,
// `int` or `int`, a number `number`, `float` or `double`, and the rest a string. `line-height`, a name or a factor,
// is `string | number` in TypeScript, `str | float` in Python, `string|float` in PHP and `LineHeight` in C#, a struct
// that converts from either. The gradients of the tokens are generated from `<gradient>` and `<stop>` too.
//
// A schema attribute marked use="required" is required in every SDK, by each language's own means: a
// required property in TypeScript, a required `init` property in C#, a field without a default in Python
// and a constructor parameter without a default in PHP. Data-binding attributes (data-*) are for XML
// templates and are left out.
//
// Python's fields are keyword-only: they follow the schema's order, so a new attribute in the middle of an
// element would move the positional arguments after it.
//
// The constants are strings, so a value goes straight into an attribute: type: FieldType.Text in Node,
// FieldAttr(type=FieldType.TEXT) in Python.
//
// It also writes the core's table of the attributes of every element and their types, src/core/src/schema_attrs.rs,
// which the engine checks a document against on both of its paths, XML and the kit tree.
//
// Usage: node scripts/gen-sdk-api.mjs                      write the core's table and the files of the four SDKs under repos/
//        node scripts/gen-sdk-api.mjs --check              list the files that differ, exit 1 if any do
//        node scripts/gen-sdk-api.mjs --check --sdk node=.  only the SDK in the given folder (an SDK repository's CI)
//        node scripts/gen-sdk-api.mjs --check --core       only the core's table (the core's CI, which has no SDKs)

import { mkdirSync, readFileSync, writeFileSync, existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { sdkRepositories } from './lib/sdk-repositories.mjs';
import { loadSchema } from './lib/xsd.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const schema = loadSchema(path.join(root, 'schema/lpdf.xsd'));
const check = process.argv.includes('--check');
const coreOnly = process.argv.includes('--core');
const { repositories, selected } = coreOnly ? { repositories: {}, selected: [] } : sdkRepositories(root, process.argv);
const CORE_TABLE = path.join(root, 'src/core/src/schema_attrs.rs');

const SDK = {
    dotnet: path.join(repositories.dotnet ?? 'repos/lpdf-dotnet', 'src'),
    node: path.join(repositories.node ?? 'repos/lpdf-js', 'src'),
    python: path.join(repositories.python ?? 'repos/lpdf-python', 'src/lpdf'),
    php: path.join(repositories.php ?? 'repos/lpdf-php', 'src'),
};

// The rules a `<markdown-styles>` holds, each an element of its own in the schema.
const MARKDOWN_RULES = ['h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'p', 'code', 'quote', 'li', 'a', 'strong', 'em', 'del', 'table', 'th', 'td'];

// `where` is how the schema declares the element (see loadSchema); `area` is the folder it lives in.
const CLASSES = [
    ...['column', 'row', 'grid', 'list', 'li', 'frame', 'link', 'divider', 'span', 'code-span', 'img', 'shape', 'barcode',
        'text', 'code', 'td', 'thead', 'tr', 'table', 'field', 'bookmark'].map((el) => ({ cls: `${pascal(el)}Attr`, el, where: 'global', area: 'layout' })),
    { cls: 'RegionAttr', el: 'region', where: 'layout', area: 'layout' },
    // A markdown, its styles and the rules of its styles, which share names with elements of the layout: a class
    // of each, named `Markdown` and the element's name, in an area of their own.
    { cls: 'MarkdownAttr', el: 'markdown', where: 'global', area: 'markdown' },
    { cls: 'MarkdownStylesAttr', el: 'markdown-styles', where: 'markdown', area: 'markdown' },
    ...MARKDOWN_RULES.map((el) => ({ cls: `Markdown${pascal(el)}Attr`, el, where: 'markdown', area: 'markdown' })),
    { cls: 'LayerAttr', el: 'layer', where: 'global', area: 'canvas' },
    ...['rect', 'circle', 'ellipse', 'line', 'path', 'text-at', 'img-at', 'barcode-at']
        .map((el) => ({ cls: `${pascal(el)}Attr`, el, where: 'global', area: 'canvas' })),
    { cls: 'LayoutAttr', el: 'layout', where: 'global', area: 'kit' },
    { cls: 'FontAttr', el: 'font', where: 'global', area: 'kit' },
    { cls: 'ImageAttr', el: 'image', where: 'global', area: 'kit' },
    { cls: 'FileAttr', el: 'file', where: 'global', area: 'kit' },
    { cls: 'ProfileAttr', el: 'profile', where: 'global', area: 'kit' },
];

// Where the classes of an area live in each SDK.
const PLACES = {
    layout: { dotnet: 'Lpdf.Layout', php: 'Lpdf\\Layout', folder: 'Layout', python: 'layout' },
    canvas: { dotnet: 'Lpdf.Canvas', php: 'Lpdf\\Canvas', folder: 'Canvas', python: 'canvas' },
    kit: { dotnet: 'Lpdf.Kit', php: 'Lpdf\\Kit', folder: 'Kit', python: 'kit' },
    markdown: { dotnet: 'Lpdf.Markdown', php: 'Lpdf\\Markdown', folder: 'Markdown', python: 'markdown' },
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
    return `Attributes of the \`${spec.el}\` element${spec.area === 'canvas' && spec.el !== 'layer' ? ' in a layer' : ''}.`;
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

// The type of an attribute in each language, by its kind (see loadSchema).
const TYPES = {
    node: { boolean: 'boolean', integer: 'number', decimal: 'number', 'name-or-number': 'string | number', string: 'string' },
    python: { boolean: 'bool', integer: 'int', decimal: 'float', 'name-or-number': 'str | float', string: 'str' },
    php: { boolean: 'bool', integer: 'int', decimal: 'float', 'name-or-number': 'string|float', string: 'string' },
    dotnet: { boolean: 'bool', integer: 'int', decimal: 'double', 'name-or-number': 'LineHeight', string: 'string' },
};

function typeOf(language, attribute) {
    const { kind } = attribute.type;
    // LineHeight is the C# type of line-height alone; another attribute of a name or a number needs a type of its own.
    if (kind === 'name-or-number' && attribute.name !== 'line-height') {
        throw new Error(`${attribute.name} is a name or a number, which only line-height has a type for`);
    }
    return TYPES[language][kind];
}

function phpParameter(attribute, name) {
    const type = typeOf('php', attribute);
    if (attribute.required) return `public ${type} $${name},`;
    return type.includes('|') ? `public ${type}|null $${name} = null,` : `public ?${type} $${name} = null,`;
}

// ── .NET ──────────────────────────────────────────────────────────────────────

function dotnet(spec, element) {
    const lines = [...HEADER.map((h) => `// ${h}`), `namespace ${PLACES[spec.area].dotnet};`, '', '#pragma warning disable CS1591',
        `/// <summary>${xmlDoc(summary(spec, element))}</summary>`, `public sealed record ${spec.cls}`, '{'];
    ordered(element).forEach((a, i) => {
        if (a.doc) lines.push(...(i > 0 ? [''] : []), `    /// <summary>${xmlDoc(a.doc)}</summary>`);
        lines.push(a.required
            ? `    public required ${typeOf('dotnet', a)} ${pascal(a.name)} { get; init; }`
            : `    public ${typeOf('dotnet', a)}? ${pascal(a.name)} { get; init; }`);
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
        lines.push(`        ${phpParameter(a, camel(a.name))}`);
    }
    lines.push('    ) {}', '}', '');
    return { file: path.join(SDK.php, PLACES[spec.area].folder, `${spec.cls}.php`), text: lines.join('\n') };
}

// ── Python ────────────────────────────────────────────────────────────────────

function python(spec, element) {
    const lines = [...HEADER.map((h) => `# ${h}`), 'from __future__ import annotations', '', 'from dataclasses import dataclass', '', '',
        '@dataclass(frozen=True, kw_only=True)', `class ${spec.cls}:`];
    const doc = wrap(summary(spec, element), 92, '    ');
    lines.push(doc.length === 1 ? `    """${doc[0].trim()}"""` : ['    """', ...doc, '    """'].join('\n'), '');
    for (const a of ordered(element)) {
        if (a.doc) lines.push(...wrap(a.doc, 96, '    # '));
        lines.push(a.required ? `    ${snake(a.name)}: ${typeOf('python', a)}` : `    ${snake(a.name)}: ${typeOf('python', a)} | None = None`);
    }
    lines.push('');
    return { file: path.join(SDK.python, PLACES[spec.area].python, `${snakeFromPascal(spec.cls)}.py`), text: lines.join('\n') };
}

// ── Node ──────────────────────────────────────────────────────────────────────

function nodeInterface(spec, element) {
    const lines = ['/**', ...wrap(summary(spec, element), 100, ' * '), ' */', `export interface ${spec.cls} {`];
    for (const a of ordered(element)) {
        if (a.doc) lines.push(...(a.doc.length > 80 ? ['  /**', ...wrap(a.doc, 100, '   * '), '   */'] : [`  /** ${a.doc} */`]));
        lines.push(`  ${camel(a.name)}${a.required ? '' : '?'}: ${typeOf('node', a)};`);
    }
    lines.push('}', '');
    return lines.join('\n');
}

// ── Gradients ─────────────────────────────────────────────────────────────────

// A gradient of the tokens and its stops: the attributes of `<gradient>` but `name`, which is its key in the map of
// gradients, and its `<stop>` children as `stops`. JS has interfaces and Python TypedDicts, since a field can't be
// named `from` there; PHP and C# have classes.
const STOPS_DOC = 'The colors of the gradient, for more than two: each at an offset, in increasing order. Give these, or from and to.';

function gradientParts() {
    const gradient = schema.element('gradient');
    const stop = schema.element('stop');
    return {
        doc: gradient.doc,
        attributes: ordered(gradient).filter((a) => a.name !== 'name'),
        stopDoc: 'One color of a gradient, at an offset along it.',
        stopAttributes: ordered(stop),
    };
}

function gradientFiles() {
    const { doc, attributes, stopDoc, stopAttributes } = gradientParts();
    const files = [];

    // .NET: records in Lpdf.Kit.
    const record = (name, summary, members, extra) => {
        const lines = [...HEADER.map((h) => `// ${h}`), 'namespace Lpdf.Kit;', '', '#pragma warning disable CS1591',
            `/// <summary>${xmlDoc(summary)}</summary>`, `public sealed record ${name}`, '{'];
        members.forEach((a, i) => {
            if (a.doc) lines.push(...(i > 0 ? [''] : []), `    /// <summary>${xmlDoc(a.doc)}</summary>`);
            lines.push(a.required
                ? `    public required ${typeOf('dotnet', a)} ${pascal(a.name)} { get; init; }`
                : `    public ${typeOf('dotnet', a)}? ${pascal(a.name)} { get; init; }`);
        });
        lines.push(...extra, '}', '');
        return { file: path.join(SDK.dotnet, 'Kit', `${name}.cs`), text: lines.join('\n') };
    };
    files.push(record('Gradient', doc, attributes,
        ['', `    /// <summary>${xmlDoc(STOPS_DOC)}</summary>`, '    public IReadOnlyList<GradientStop>? Stops { get; init; }']));
    files.push(record('GradientStop', stopDoc, stopAttributes, []));

    // PHP: readonly classes in Lpdf\Kit.
    const phpClass = (name, summary, members, extra) => {
        const lines = ['<?php', '', 'declare(strict_types=1);', '', 'namespace Lpdf\\Kit;', '', '/**',
            ...wrap(summary, 100, ' * '), ' *', ...HEADER.map((h) => ` * ${h}`), ' */', `final readonly class ${name}`, '{'];
        if (extra.length) lines.push('    /**', ...wrap(`@param list<GradientStop> $stops ${STOPS_DOC}`, 100, '     * '), '     */');
        lines.push('    public function __construct(');
        for (const a of members) {
            if (a.doc) lines.push(...(a.doc.length > 80 ? ['        /**', ...wrap(a.doc, 100, '         * '), '         */'] : [`        /** ${a.doc} */`]));
            lines.push(`        ${phpParameter(a, camel(a.name))}`);
        }
        lines.push(...extra, '    ) {}', '}', '');
        return { file: path.join(SDK.php, 'Kit', `${name}.php`), text: lines.join('\n') };
    };
    files.push(phpClass('Gradient', doc, attributes, ['        public array $stops = [],']));
    files.push(phpClass('GradientStop', stopDoc, stopAttributes, []));

    // Python: TypedDicts in lpdf.kit, the gradient in the functional form, because `from` is a keyword.
    const pyStop = [...HEADER.map((h) => `# ${h}`), 'from __future__ import annotations', '', 'from typing import TypedDict', '', '',
        'class GradientStop(TypedDict):', `    """${stopDoc}"""`, ''];
    for (const a of stopAttributes) pyStop.push(...wrap(a.doc, 96, '    # '), `    ${snake(a.name)}: ${typeOf('python', a)}`);
    pyStop.push('');
    files.push({ file: path.join(SDK.python, 'kit', 'gradient_stop.py'), text: pyStop.join('\n') });
    const pyGradient = [...HEADER.map((h) => `# ${h}`), 'from __future__ import annotations', '',
        'from typing import Required, TypedDict', '', 'from .gradient_stop import GradientStop', '',
        ...wrap(doc, 100, '# '), '# The functional form, because ``from`` is a Python keyword.', 'Gradient = TypedDict(', '    "Gradient",', '    {'];
    for (const a of attributes) {
        const type = typeOf('python', a);
        pyGradient.push(...wrap(a.doc, 92, '        # '), `        "${a.name}": ${a.required ? `Required[${type}]` : type},`);
    }
    pyGradient.push(...wrap(STOPS_DOC, 92, '        # '), '        "stops": "list[GradientStop]",', '    },', '    total=False,', ')', '');
    files.push({ file: path.join(SDK.python, 'kit', 'gradient.py'), text: pyGradient.join('\n') });

    // Node: interfaces in attrs.ts. The type of a gradient lists its values, as the hand-written one did.
    const kinds = schema.simpleTypeValues('GradientType').map((v) => `'${v}'`).join(' | ');
    const tsMember = (a, type) => [...(a.doc.length > 80 ? ['  /**', ...wrap(a.doc, 100, '   * '), '   */'] : [`  /** ${a.doc} */`]),
        `  ${camel(a.name)}${a.required ? '' : '?'}: ${type};`];
    const node = ['/**', ...wrap(stopDoc, 100, ' * '), ' */', 'export interface GradientStop {'];
    for (const a of stopAttributes) node.push(...tsMember(a, typeOf('node', a)));
    node.push('}', '', '/**', ...wrap(doc, 100, ' * '), ' */', 'export interface GradientToken {');
    for (const a of attributes) node.push(...tsMember(a, a.name === 'type' ? kinds : typeOf('node', a)));
    node.push(...['  /**', ...wrap(STOPS_DOC, 100, '   * '), '   */'], '  stops?: GradientStop[];', '}', '');
    return { files, node: node.join('\n') };
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
    { name: 'SectionType', area: 'kit', file: 'section_type', values: () => schema.simpleTypeValues('SectionType'),
      doc: 'The values of the `type` attribute of a section.' },
    { name: 'FootnoteCounter', area: 'kit', file: 'footnote_counter', values: () => schema.simpleTypeValues('FootnoteCounter'),
      doc: 'The values of the `footnote-counter` attribute of a document or a section.' },
    { name: 'Cipher', area: 'kit', file: 'cipher', values: () => schema.simpleTypeValues('Cipher'),
      doc: 'The values of the `encrypt` attribute of the output: the cipher the PDF is encrypted with.' },
    { name: 'Permission', area: 'kit', file: 'permission', values: () => schema.simpleTypeValues('Permission'),
      doc: 'The words of the `allow` attribute of the output, separated by spaces: what a reader of an encrypted PDF may do.' },
    { name: 'FileRelationship', area: 'kit', file: 'file_relationship', values: () => schema.simpleTypeValues('FileRelationship'),
      doc: 'The values of the `relationship` attribute of a file the PDF carries: how it relates to the document.' },
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

// ── Core ──────────────────────────────────────────────────────────────────────

/** A number of the schema as Rust writes it: a decimal bound always with a point. */
function rustNumber(value, kind) {
    return kind === 'integer' || !Number.isInteger(value) ? String(value) : `${value}.0`;
}

function rustBound(type, end) {
    const value = type[end];
    if (value === undefined) return 'Bound::Open';
    const exclusive = end === 'min' ? type.minExclusive : type.maxExclusive;
    return `Bound::${exclusive ? 'Exclusive' : 'Inclusive'}(${rustNumber(value, 'decimal')})`;
}

function rustKind(type) {
    const integer = (end) => (type[end] === undefined ? 'None' : `Some(${type[end]})`);
    switch (type.kind) {
        case 'boolean': return 'Kind::Boolean';
        case 'integer': return `Kind::Integer { min: ${integer('min')}, max: ${integer('max')} }`;
        case 'decimal': return `Kind::Decimal { min: ${rustBound(type, 'min')}, max: ${rustBound(type, 'max')} }`;
        case 'name-or-number':
            if (type.number !== 'decimal') throw new Error('a name or a whole number has no kind in the core');
            return `Kind::NameOrDecimal { min: ${rustBound(type, 'min')}, max: ${rustBound(type, 'max')} }`;
        default: return 'Kind::Text';
    }
}

function rustConst(element) {
    return [element.within, element.name].filter(Boolean).join('_').replace(/-/g, '_').toUpperCase();
}

/**
 * The core's table: a constant with the attributes of each element, in schema order and without the data-*
 * attributes, and `attributes_of`, which finds the element a name stands for where it is written. A name means the
 * element declared in its parent when there is one, such as a row of the tokens or a rule of a markdown's styles;
 * in a layer only a drawing primitive, so that a layout element there is reported by the parser, not by its
 * attributes; anywhere else the top-level element.
 */
function coreTable() {
    const elements = schema.allElements();
    const primitives = schema.groupElements('CanvasPrimitives');
    const lines = [
        ...HEADER.map((h) => `// ${h}`), '',
        '//! The attributes of every element of lpdf.xsd and their types.', '',
        'use crate::attr_types::{Bound, Kind};', '',
        '/// The attributes of an element, in the order the schema lists them, without the data-* attributes.',
        'pub(crate) type Attributes = &\'static [(&\'static str, Kind)];', '',
    ];
    for (const element of elements) {
        const attributes = element.attributes.filter((a) => !a.name.startsWith('data-'));
        lines.push(`const ${rustConst(element)}: Attributes = &[`);
        for (const a of attributes) lines.push(`    ("${a.name}", ${rustKind(a.type)}),`);
        lines.push('];', '');
    }
    lines.push(
        '/// The attributes of the element `name` written in the element `parent`, or `None` for an element the schema',
        '/// does not have there.',
        'pub(crate) fn attributes_of(parent: &str, name: &str) -> Option<Attributes> {',
        '    match (parent, name) {',
    );
    for (const element of elements.filter((e) => e.within)) {
        lines.push(`        ("${element.within}", "${element.name}") => Some(${rustConst(element)}),`);
    }
    for (const name of primitives) lines.push(`        ("layer", "${name}") => Some(${rustConst({ name })}),`);
    lines.push('        ("layer", _) => None,');
    for (const element of elements.filter((e) => !e.within)) {
        lines.push(`        (_, "${element.name}") => Some(${rustConst(element)}),`);
    }
    lines.push('        _ => None,', '    }', '}', '');
    return { file: CORE_TABLE, text: lines.join('\n') };
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
const gradients = gradientFiles();
generated.push(...gradients.files);
generated.push({
    file: path.join(SDK.node, 'attrs.ts'),
    text: [...HEADER.map((h) => `// ${h}`), '', ...nodeInterfaces, gradients.node].join('\n'),
});

// Only the SDKs asked for: the files of the others are not there to compare with. The core's table goes with all
// four, or alone.
const inSelected = (file) => selected.some((language) => file.startsWith(SDK[language] + path.sep));
const withCore = coreOnly || !process.argv.some((a) => a === '--sdk' || a.startsWith('--sdk='));
const outputs = [...(withCore ? [coreTable()] : []), ...generated.filter(({ file }) => inSelected(file))];

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
