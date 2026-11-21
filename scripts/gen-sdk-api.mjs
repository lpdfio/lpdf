// Writes the API that follows from lpdf.xsd into the five SDKs: the attribute classes, and the constants for
// the schema's enumerations. Every SDK gets the same classes and constants with the schema's names,
// differing only in the case rule of its language: font-size is fontSize in Node, PHP and Java, font_size in
// Python and FontSize in .NET.
//
// Each attribute has the type the schema gives it: a boolean is `boolean`, `bool` or `bool`, a whole number `number`,
// `int` or `int`, a number `number`, `float` or `double`, and the rest a string. `line-height`, a name or a factor,
// is `string | number` in TypeScript, `str | float` in Python, `string|float` in PHP, `LineHeight` in C#, a struct
// that converts from either, and two setters in Java, one for each. The spot colors of the tokens are generated from
// `<spot>` too, the gradients from `<gradient>` and `<stop>`, and the rulings from `<ruling>` and `<lines>`.
//
// A schema attribute marked use="required" is required in every SDK, by each language's own means: a
// required property in TypeScript, a required `init` property in C#, a field without a default in Python,
// a constructor parameter without a default in PHP, and in Java, whose classes have fluent setters, a check
// when the builder that takes the attributes is called. Data-binding attributes (data-*) are for XML
// templates and are left out.
//
// Python's fields are keyword-only: they follow the schema's order, so a new attribute in the middle of an
// element would move the positional arguments after it.
//
// The constants are strings, so a value goes straight into an attribute: type: FieldType.Text in Node,
// FieldAttr(type=FieldType.TEXT) in Python, new FieldAttr().type(FieldType.TEXT) in Java. There is one for each closed
// enumeration of the schema, derived from it (scripts/lib/constants.mjs) with the doc comment of the type, so an
// enumeration that is added gets one by running this script. In TypeScript the attribute takes the constant's type:
// `orientation?: Orientation`, so a value that is none of its own doesn't compile; `size` also takes a width and a
// height. The other four SDKs take a string.
//
// It also writes the core's table of the attributes of every element and their types, src/core/src/schema_attrs.rs,
// which the engine checks a document against on both of its paths, XML and the kit tree, and which `lpdf codegen`
// reads to write a constant where the schema has one: the constants' members and the constants each attribute takes.
//
// Usage: node scripts/gen-sdk-api.mjs                      write the core's table and the files of the five SDKs under repos/
//        node scripts/gen-sdk-api.mjs --check              list the files that differ, exit 1 if any do
//        node scripts/gen-sdk-api.mjs --check --sdk node=.  only the SDK in the given folder (an SDK repository's CI)
//        node scripts/gen-sdk-api.mjs --check --core       only the core's table (the core's CI, which has no SDKs)

import { mkdirSync, readFileSync, writeFileSync, existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { sdkRepositories } from './lib/sdk-repositories.mjs';
import { loadSchema } from './lib/xsd.mjs';
import { constantsOf, shapeOf, STAMP_CONSTANTS, typeScriptOf } from './lib/constants.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const schema = loadSchema(path.join(root, 'schema/lpdf.xsd'));
const CONSTANTS = constantsOf(schema);
const check = process.argv.includes('--check');
const coreOnly = process.argv.includes('--core');
const { repositories, selected } = coreOnly ? { repositories: {}, selected: [] } : sdkRepositories(root, process.argv);
const CORE_TABLE = path.join(root, 'src/core/src/schema_attrs.rs');

const SDK = {
    dotnet: path.join(repositories.dotnet ?? 'repos/lpdf-dotnet', 'src'),
    node: path.join(repositories.node ?? 'repos/lpdf-js', 'src'),
    python: path.join(repositories.python ?? 'repos/lpdf-python', 'src/lpdf'),
    php: path.join(repositories.php ?? 'repos/lpdf-php', 'src'),
    // One package, io.lpdf, for the whole API, so that `import io.lpdf.*;` brings every class a document names.
    java: path.join(repositories.java ?? 'repos/lpdf-java', 'src/main/java/io/lpdf'),
};

// The rules a `<markdown-styles>` holds, each an element of its own in the schema.
const MARKDOWN_RULES = ['h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'p', 'code', 'quote', 'li', 'hr', 'a', 'strong', 'em', 'del', 'table', 'th', 'td'];

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

/** The words of an attribute's name, or of a value such as `width height`, which holds a space. */
function words(name) { return name.split(/[-\s]/); }
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
    // A name or a number has a setter for each, which javaSetters makes of the two types.
    java: { boolean: 'boolean', integer: 'int', decimal: 'double', 'name-or-number': 'String|double', string: 'String' },
};

/** The constants' types that the interfaces of attrs.ts name, which it imports. */
const NODE_TYPES_USED = new Set();

function typeOf(language, attribute) {
    const { kind } = attribute.type;
    // LineHeight is the C# type of line-height alone; another attribute of a name or a number needs a type of its own.
    if (kind === 'name-or-number' && attribute.name !== 'line-height') {
        throw new Error(`${attribute.name} is a name or a number, which only line-height has a type for`);
    }
    if (language === 'node') {
        const shape = shapeOf(schema, CONSTANTS, attribute.typeName);
        if (shape) {
            const type = typeScriptOf(shape);
            for (const part of type.split(' | ')) if (part !== 'string') NODE_TYPES_USED.add(part);
            return type;
        }
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

// ── Java ──────────────────────────────────────────────────────────────────────

// A class of attributes has a fluent setter for each, which returns the class, and its constructor names the element
// and the attributes it needs, which `Attributes` checks when a builder takes them: `new TableAttr().cols("1fr auto")`.

function javaText(text) {
    return text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/@/g, '&#64;')
        .replace(/\*\//g, '*&#47;').replace(/`([^`]+)`/g, '<code>$1</code>');
}

/** A Javadoc comment of `text` at `indent`: one line when it fits in 120 columns, else wrapped. */
function javadoc(text, indent) {
    const one = `${indent}/** ${javaText(text)} */`;
    return one.length <= 120 ? [one] : [`${indent}/**`, ...wrap(javaText(text), 120, `${indent} * `), `${indent} */`];
}

const JAVA_HEADER = [...HEADER.map((h) => `// ${h}`), 'package io.lpdf;', ''];

/** The setters of an attribute: one, or one for each type of a name or a number. */
function javaSetters(cls, a) {
    const name = camel(a.name);
    const doc = a.doc ? javadoc(a.doc, '    ') : [];
    return typeOf('java', a).split('|').flatMap((type) => [
        '', ...doc, `    public ${cls} ${name}(${type} ${name}) {`, `        set("${a.name}", ${name});`, '        return this;', '    }',
    ]);
}

/** A class of attributes named `cls`, of the element `el`, with `extra` lines after the setters of `attributes`. */
function javaClass(cls, el, doc, attributes, extra = [], required = attributes.filter((a) => a.required).map((a) => a.name)) {
    const lines = [...JAVA_HEADER, ...javadoc(doc, ''), `public final class ${cls} extends Attributes {`,
        '    /** Attributes with none set. */', `    public ${cls}() {`,
        `        super(${[el, ...required].map((n) => `"${n}"`).join(', ')});`, '    }'];
    for (const a of attributes) lines.push(...javaSetters(cls, a));
    lines.push(...extra, '}', '');
    return { file: path.join(SDK.java, `${cls}.java`), text: lines.join('\n') };
}

function java(spec, element) {
    return javaClass(spec.cls, spec.el, summary(spec, element), ordered(element));
}

// ── Spot colors ───────────────────────────────────────────────────────────────

// A spot color of the tokens: the attributes of `<spot>` but `name`, which is its key in the map of spot colors. PHP,
// C# and Java have classes, Python a TypedDict, and JS an interface, `SpotToken`, as for a gradient.
function spotFiles() {
    const spot = schema.element('spot');
    const attributes = ordered(spot).filter((a) => a.name !== 'name');
    const files = [kitRecord('Spot', spot.doc, attributes, []), kitPhpClass('Spot', spot.doc, attributes, null)];

    const python = [...HEADER.map((h) => `# ${h}`), 'from __future__ import annotations', '',
        'from typing import Required, TypedDict', '', '', 'class Spot(TypedDict, total=False):'];
    python.push(...['    """', ...wrap(spot.doc, 92, '    '), '    """'], '');
    for (const a of attributes) {
        const type = typeOf('python', a);
        python.push(...wrap(a.doc, 96, '    # '), `    ${snake(a.name)}: ${a.required ? `Required[${type}]` : type}`);
    }
    python.push('');
    files.push({ file: path.join(SDK.python, 'kit', 'spot.py'), text: python.join('\n') });

    files.push(javaClass('Spot', 'spot', spot.doc, attributes));

    const node = ['/**', ...wrap(spot.doc, 100, ' * '), ' */', 'export interface SpotToken {'];
    for (const a of attributes) node.push(...tsMember(a, typeOf('node', a)));
    node.push('}', '');
    return { files, node: node.join('\n') };
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

/** A record of a token in Lpdf.Kit, in .NET: `members` as properties, then the lines of `extra`. */
function kitRecord(name, summary, members, extra) {
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
}

/**
 * A readonly class of a token in Lpdf\Kit, in PHP: `members` as constructor parameters, with `list`, a list of
 * children (`{ name, type, doc, parameter }`), before them when it is required and after them when it is not.
 */
function kitPhpClass(name, summary, members, list) {
    const lines = ['<?php', '', 'declare(strict_types=1);', '', 'namespace Lpdf\\Kit;', '', '/**',
        ...wrap(summary, 100, ' * '), ' *', ...HEADER.map((h) => ` * ${h}`), ' */', `final readonly class ${name}`, '{'];
    if (list) lines.push('    /**', ...wrap(`@param list<${list.type}> $${list.name} ${list.doc}`, 100, '     * '), '     */');
    lines.push('    public function __construct(');
    if (list?.required) lines.push(`        ${list.parameter}`);
    for (const a of members) {
        if (a.doc) lines.push(...(a.doc.length > 80 ? ['        /**', ...wrap(a.doc, 100, '         * '), '         */'] : [`        /** ${a.doc} */`]));
        lines.push(`        ${phpParameter(a, camel(a.name))}`);
    }
    if (list && !list.required) lines.push(`        ${list.parameter}`);
    lines.push('    ) {}', '}', '');
    return { file: path.join(SDK.php, 'Kit', `${name}.php`), text: lines.join('\n') };
}

/** A member of a TypeScript interface, with its doc comment. */
function tsMember(a, type) {
    return [...(a.doc.length > 80 ? ['  /**', ...wrap(a.doc, 100, '   * '), '   */'] : [`  /** ${a.doc} */`]),
        `  ${camel(a.name)}${a.required ? '' : '?'}: ${type};`];
}

function gradientFiles() {
    const { doc, attributes, stopDoc, stopAttributes } = gradientParts();
    const files = [];

    // .NET: records in Lpdf.Kit.
    files.push(kitRecord('Gradient', doc, attributes,
        ['', `    /// <summary>${xmlDoc(STOPS_DOC)}</summary>`, '    public IReadOnlyList<GradientStop>? Stops { get; init; }']));
    files.push(kitRecord('GradientStop', stopDoc, stopAttributes, []));

    // PHP: readonly classes in Lpdf\Kit.
    files.push(kitPhpClass('Gradient', doc, attributes,
        { name: 'stops', type: 'GradientStop', doc: STOPS_DOC, required: false, parameter: 'public array $stops = [],' }));
    files.push(kitPhpClass('GradientStop', stopDoc, stopAttributes, null));

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

    // Java: classes of attributes, the stops a list of their own.
    files.push(javaClass('Gradient', 'gradient', doc, attributes, [
        '', ...javadoc(STOPS_DOC, '    '), '    public Gradient stops(java.util.List<GradientStop> stops) {',
        '        set("stops", stops == null ? null : java.util.List.copyOf(stops));', '        return this;', '    }',
    ]));
    files.push(javaClass('GradientStop', 'stop', stopDoc, stopAttributes));

    // Node: interfaces in attrs.ts.
    const node = ['/**', ...wrap(stopDoc, 100, ' * '), ' */', 'export interface GradientStop {'];
    for (const a of stopAttributes) node.push(...tsMember(a, typeOf('node', a)));
    node.push('}', '', '/**', ...wrap(doc, 100, ' * '), ' */', 'export interface GradientToken {');
    for (const a of attributes) node.push(...tsMember(a, typeOf('node', a)));
    node.push(...['  /**', ...wrap(STOPS_DOC, 100, '   * '), '   */'], '  stops?: GradientStop[];', '}', '');
    return { files, node: node.join('\n') };
}

// ── Rulings ───────────────────────────────────────────────────────────────────

// A ruling of the tokens and its sets of lines: the attributes of `<ruling>` but `name`, which is its key in the map of
// rulings, and its `<lines>` children as `lines`, of which it has at least one. PHP and C# have classes and JS
// interfaces, whose members are in each language's case, which the SDK turns back into the schema's names. Python has
// TypedDicts with the schema's names, the lines in the functional form, since `step-x` can't be a field.
const LINES_DOC = 'The sets of lines, at least one, drawn in this order: a later set paints over an earlier one.';

function rulingParts() {
    const ruling = schema.element('ruling');
    const lines = schema.element('lines');
    return {
        doc: ruling.doc,
        attributes: ordered(ruling).filter((a) => a.name !== 'name'),
        linesDoc: lines.doc,
        linesAttributes: ordered(lines),
    };
}

function rulingFiles() {
    const { doc, attributes, linesDoc, linesAttributes } = rulingParts();
    const files = [];

    // .NET: records in Lpdf.Kit.
    files.push(kitRecord('Ruling', doc, attributes,
        ['', `    /// <summary>${xmlDoc(LINES_DOC)}</summary>`, '    public required IReadOnlyList<RulingLines> Lines { get; init; }']));
    files.push(kitRecord('RulingLines', linesDoc, linesAttributes, []));

    // PHP: readonly classes in Lpdf\Kit.
    files.push(kitPhpClass('Ruling', doc, attributes,
        { name: 'lines', type: 'RulingLines', doc: LINES_DOC, required: true, parameter: 'public array $lines,' }));
    files.push(kitPhpClass('RulingLines', linesDoc, linesAttributes, null));

    // Python: TypedDicts in lpdf.kit.
    const pyLines = [...HEADER.map((h) => `# ${h}`), 'from __future__ import annotations', '', 'from typing import TypedDict', '',
        ...wrap(linesDoc, 100, '# '), '# The functional form, because ``step-x`` is not a Python name.', 'RulingLines = TypedDict(',
        '    "RulingLines",', '    {'];
    for (const a of linesAttributes) pyLines.push(...wrap(a.doc, 92, '        # '), `        "${a.name}": ${typeOf('python', a)},`);
    pyLines.push('    },', '    total=False,', ')', '');
    files.push({ file: path.join(SDK.python, 'kit', 'ruling_lines.py'), text: pyLines.join('\n') });
    const pyRuling = [...HEADER.map((h) => `# ${h}`), 'from __future__ import annotations', '',
        'from typing import Required, TypedDict', '', 'from .ruling_lines import RulingLines', '', '',
        'class Ruling(TypedDict, total=False):'];
    pyRuling.push(...['    """', ...wrap(doc, 92, '    '), '    """'], '');
    for (const a of attributes) pyRuling.push(...wrap(a.doc, 96, '    # '), `    ${snake(a.name)}: ${typeOf('python', a)}`);
    pyRuling.push(...wrap(LINES_DOC, 96, '    # '), '    lines: Required[list[RulingLines]]', '');
    files.push({ file: path.join(SDK.python, 'kit', 'ruling.py'), text: pyRuling.join('\n') });

    // Java: classes of attributes, the lines a list of their own, which a ruling needs.
    files.push(javaClass('Ruling', 'ruling', doc, attributes, [
        '', ...javadoc(LINES_DOC, '    '), '    public Ruling lines(java.util.List<RulingLines> lines) {',
        '        set("lines", lines == null ? null : java.util.List.copyOf(lines));', '        return this;', '    }',
    ], [...attributes.filter((a) => a.required).map((a) => a.name), 'lines']));
    files.push(javaClass('RulingLines', 'lines', linesDoc, linesAttributes));

    // Node: interfaces in attrs.ts.
    const node = ['/**', ...wrap(linesDoc, 100, ' * '), ' */', 'export interface RulingLines {'];
    for (const a of linesAttributes) node.push(...tsMember(a, typeOf('node', a)));
    node.push('}', '', '/**', ...wrap(doc, 100, ' * '), ' */', 'export interface RulingToken {');
    for (const a of attributes) node.push(...tsMember(a, typeOf('node', a)));
    node.push(...['  /**', ...wrap(LINES_DOC, 100, '   * '), '   */'], '  lines: RulingLines[];', '}', '');
    return { files, node: node.join('\n') };
}

// ── Constants ─────────────────────────────────────────────────────────────────

// One constant for each closed enumeration of the schema (scripts/lib/constants.mjs), except the few it excludes with
// a reason. `area` is the folder of the .NET and PHP file: where the attributes that take it are, or `shared` when
// they are in more than one place, as the `page` of a layer and of a region are.

const AREAS = {
    layout: { folder: 'Layout', dotnet: 'Lpdf.Layout', php: 'Lpdf\\Layout' },
    canvas: { folder: 'Canvas', dotnet: 'Lpdf.Canvas', php: 'Lpdf\\Canvas' },
    kit: { folder: 'Kit', dotnet: 'Lpdf.Kit', php: 'Lpdf\\Kit' },
    markdown: { folder: 'Markdown', dotnet: 'Lpdf.Markdown', php: 'Lpdf\\Markdown' },
    shared: { folder: 'Shared', dotnet: 'Lpdf.Shared', php: 'Lpdf\\Shared' },
    engine: { folder: 'Engine', dotnet: 'Lpdf.Engine', php: 'Lpdf\\Engine' },
};

/** The area of the classes of an element, as CLASSES gives it; the document, its sections, tokens and assets are `kit`. */
function areaOfElement(element) {
    if (element.within === 'markdown-styles' || element.within === 'markdown') return 'markdown';
    const spec = CLASSES.find((c) => c.el === element.name && c.where !== 'markdown');
    return spec ? spec.area : 'kit';
}

/** The area of a constant: that of every element with an attribute of its type, or `shared` for more than one. */
function areaOfConstant(constant) {
    const areas = new Set();
    for (const element of schema.allElements()) {
        for (const a of element.attributes) {
            // A list takes its items' constant as words of a string, such as `allow` of the output.
            const takes = [a.typeName, schema.listItemType(a.typeName)];
            if (takes.some((type) => shapeOf(schema, CONSTANTS, type)?.constants.includes(constant.name))) areas.add(areaOfElement(element));
        }
    }
    if (areas.size === 0) throw new Error(`no attribute takes the constant ${constant.name}`);
    return areas.size === 1 ? [...areas][0] : 'shared';
}
for (const constant of CONSTANTS) constant.area = areaOfConstant(constant);

/** The constants the SDKs have: the schema's, and the stamp's. The core's table has the schema's alone. */
const SDK_CONSTANTS = [...CONSTANTS, ...STAMP_CONSTANTS];

/**
 * The files of a constant: .NET, PHP and Java have a class of its own, in the .NET and PHP area it belongs to, and Python
 * and Node have one file for all constants (`constantsFiles`).
 */
function constantsFor(constant) {
    const { name, doc, members } = constant;
    const area = AREAS[constant.area];

    const dotnetLines = [
        ...HEADER.map((h) => `// ${h}`), `namespace ${area.dotnet};`, '', '#pragma warning disable CS1591',
        `/// <summary>${xmlDoc(doc)}</summary>`, `public static class ${name}`, '{',
        ...members.map((m) => `    public const string ${m.pascal} = "${m.value}";`), '}', '',
    ];

    // Untyped constants: typed class constants need PHP 8.3, and the PHP SDK runs on 8.2.
    const phpLines = [
        '<?php', '', 'declare(strict_types=1);', '', `namespace ${area.php};`, '', '/**',
        ...wrap(doc, 100, ' * '), ' *', ...HEADER.map((h) => ` * ${h}`), ' */', `final class ${name}`, '{',
        ...members.map((m) => `    public const ${m.pascal} = '${m.value}';`), '}', '',
    ];

    // Java's constants are in its own case, as Python's are.
    const javaLines = [
        ...JAVA_HEADER, ...javadoc(doc, ''), `public final class ${name} {`, `    private ${name}() {`, '    }', '',
        ...members.map((m) => `    public static final String ${m.snake} = "${m.value}";`), '}', '',
    ];

    return [
        { file: path.join(SDK.dotnet, area.folder, `${name}.cs`), text: dotnetLines.join('\n') },
        { file: path.join(SDK.php, area.folder, `${name}.php`), text: phpLines.join('\n') },
        { file: path.join(SDK.java, `${name}.java`), text: javaLines.join('\n') },
    ];
}

/** A width and a height, as the schema's `PageSizeCustom` writes them; TypeScript has a template literal type for it. */
const PAGE_SIZE_CUSTOM = { pattern: '\\d+(\\.\\d+)?(pt|mm|in) \\d+(\\.\\d+)?(pt|mm|in)',
    typeScript: "`${number}${'pt' | 'mm' | 'in'} ${number}${'pt' | 'mm' | 'in'}`",
    doc: 'A width and a height, such as `100pt 200pt`: each a number and `pt`, `mm` or `in`.' };

/** The constants of Python and Node, each in one file: a `StrEnum` for each in `lpdf/constants.py`, and `constants.ts`. */
function constantsFiles() {
    const pythonLines = [...HEADER.map((h) => `# ${h}`), 'from enum import StrEnum', '', '__all__ = [', ...SDK_CONSTANTS.map((c) => `    "${c.name}",`), ']', ''];
    for (const { name, doc, members } of SDK_CONSTANTS) {
        pythonLines.push('', `class ${name}(StrEnum):`, `    """${doc}"""`, '', ...members.map((m) => `    ${m.snake} = "${m.value}"`), '');
    }

    // The pattern of the custom size is written once in the xsd and once above: they have to agree.
    const patterns = schema.patternsOf('PageSizeCustom');
    if (patterns.length !== 1 || patterns[0] !== PAGE_SIZE_CUSTOM.pattern) {
        throw new Error(`the schema's PageSizeCustom is ${patterns.join(' | ')}, which PAGE_SIZE_CUSTOM in gen-sdk-api.mjs doesn't match`);
    }
    const nodeLines = [...HEADER.map((h) => `// ${h}`), ''];
    for (const { name, doc, members } of SDK_CONSTANTS) {
        nodeLines.push(`/** ${doc} */`, `export const ${name} = {`, ...members.map((m) => `  ${m.pascal}: '${m.value}',`), '} as const;', '',
            `export type ${name} = (typeof ${name})[keyof typeof ${name}];`, '');
        if (name === 'PageSize') nodeLines.push(`/** ${PAGE_SIZE_CUSTOM.doc} */`, `export type PageSizeCustom = ${PAGE_SIZE_CUSTOM.typeScript};`, '');
    }
    return [
        { file: path.join(SDK.python, 'constants.py'), text: pythonLines.join('\n') },
        { file: path.join(SDK.node, 'constants.ts'), text: nodeLines.join('\n') },
    ];
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

function rustKind(attribute) {
    const { type } = attribute;
    const integer = (end) => (type[end] === undefined ? 'None' : `Some(${type[end]})`);
    // A string that takes the values of a constant: the code generator writes the constant in its place.
    const shape = shapeOf(schema, CONSTANTS, attribute.typeName);
    if (shape && type.kind === 'string') return `Kind::Constants(&[${shape.constants.map((c) => `"${c}"`).join(', ')}])`;
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
        'use crate::attr_types::{Bound, Constant, Kind, Member};', '',
        '/// The attributes of an element, in the order the schema lists them, without the data-* attributes.',
        'pub(crate) type Attributes = &\'static [(&\'static str, Kind)];', '',
    ];
    for (const element of elements) {
        const attributes = element.attributes.filter((a) => !a.name.startsWith('data-'));
        lines.push(`const ${rustConst(element)}: Attributes = &[`);
        for (const a of attributes) lines.push(`    ("${a.name}", ${rustKind(a)}),`);
        lines.push('];', '');
    }
    lines.push(
        '/// The constants of the SDKs, one for each closed enumeration of the schema: the name, the area it is in (the .NET',
        '/// and PHP namespace) and its members, each a value with its name in Pascal case and in upper snake case.',
        '#[allow(dead_code, reason = "only codegen asks, and the WASI binary and the native library have no codegen")]',
        'pub(crate) const CONSTANTS: &[Constant] = &[',
    );
    for (const c of CONSTANTS) {
        lines.push(`    Constant { name: "${c.name}", area: "${c.area}", members: &[`);
        for (const m of c.members) lines.push(`        Member { value: "${m.value}", pascal: "${m.pascal}", snake: "${m.snake}" },`);
        lines.push('    ] },');
    }
    lines.push('];', '');
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
    generated.push(dotnet(spec, element), php(spec, element), python(spec, element), java(spec, element));
    nodeInterfaces.push(nodeInterface(spec, element));
}
for (const constant of SDK_CONSTANTS) generated.push(...constantsFor(constant));
generated.push(...constantsFiles());
const spots = spotFiles();
generated.push(...spots.files);
const gradients = gradientFiles();
generated.push(...gradients.files);
const rulings = rulingFiles();
generated.push(...rulings.files);
generated.push({
    file: path.join(SDK.node, 'attrs.ts'),
    text: [...HEADER.map((h) => `// ${h}`), '', `import type { ${[...NODE_TYPES_USED].sort().join(', ')} } from './constants';`, '',
        ...nodeInterfaces, spots.node, gradients.node, rulings.node].join('\n'),
});

// Only the SDKs asked for: the files of the others are not there to compare with. The core's table goes with all
// five, or alone.
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
