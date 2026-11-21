// The constants the SDKs name for the schema's enumerations, derived from lpdf.xsd: one for each closed enumeration, so
// a new enumeration gets one without a table to add it to. Shared by `gen-sdk-api.mjs`, which writes them, and
// `check-sdk-parity.mjs`, which checks that the hand-written TypeScript types and the entry points agree with them.
//
// A constant is named after its schema type, with `Named` dropped from the named part of a union (`PageSizeNamed` is the
// constant `PageSize`, whose union with `PageSizeCustom` is the attribute's type). Its members are the values in each
// language's case: `Landscape`, or `LANDSCAPE` in Python and Java. A value that starts with a digit takes the last word
// of the constant's name as a prefix, which none of the five languages could do without (`PdfALevel.Level2a`, `LEVEL_2A`).

/** The closed enumerations that get no constant, and why. */
export const EXCLUDED = {
    TokenScale: 'the slots xs to xxl, the most written strings of the API, stay strings: an alias or a length is always another valid form',
    HeightMode: 'full and fill are two keywords of a union with lengths, slots and aliases, not a choice a person makes by name',
    WidthMode: 'fill is one keyword of a union with lengths, slots and aliases, not a choice a person makes by name',
};

const words = (name) => name.split(/[-\s]/);
const pascal = (name) => words(name).map((w) => w[0].toUpperCase() + w.slice(1)).join('');

/** The last word of a constant's name: `PdfALevel` is `Pdf`, `A`, `Level`. It prefixes a value that starts with a digit. */
function lastWord(name) {
    return name.match(/[A-Z][a-z0-9]*/g)?.at(-1) ?? name;
}

/** The member of `value` in a constant named `name`, in Pascal case and in upper snake case. */
export function memberNames(name, value) {
    const prefix = /^\d/.test(value) ? lastWord(name) : '';
    return {
        pascal: prefix + pascal(value),
        snake: (prefix ? `${prefix.toUpperCase()}_` : '') + words(value).join('_').toUpperCase(),
    };
}

/**
 * The constants of the schema, in schema order: `{ name, type, values, doc, members }`, where `members` are the values
 * with their names (`{ value, pascal, snake }`). Refuses what the languages could not write: an enumeration
 * written inline on an attribute, a type with no documentation, which is the constant's doc comment, two constants of one name,
 * a member name twice in a constant.
 */
export function constantsOf(schema) {
    // An enumeration written inline on an attribute has no name to make a constant of, and would get none without a word.
    const inline = schema.inlineEnumerations?.() ?? [];
    if (inline.length) {
        throw new Error(`${inline.join(', ')} declare${inline.length === 1 ? 's' : ''} an enumeration inline: give it a named xs:simpleType and a type=, so the SDKs get a constant`);
    }
    const constants = [];
    for (const { type, values, doc } of schema.enumerations()) {
        if (type in EXCLUDED) continue;
        const name = type.endsWith('Named') ? type.slice(0, -'Named'.length) : type;
        if (!doc) throw new Error(`the schema's type ${type} is a closed enumeration and needs an xs:documentation: it is its constant's doc comment`);
        if (constants.some((c) => c.name === name)) throw new Error(`two closed enumerations make a constant ${name}`);
        const members = values.map((value) => ({ value, ...memberNames(name, value) }));
        for (const key of ['pascal', 'snake']) {
            const seen = new Set();
            for (const member of members) {
                if (seen.has(member[key])) throw new Error(`${name} has two members called ${member[key]}`);
                seen.add(member[key]);
            }
        }
        constants.push({ name, type, values, doc, members });
    }
    return constants;
}

/**
 * What an attribute's type draws on: `{ constants, open }`, the constants whose values it takes and the types it also
 * takes that no constant names (`PageSizeCustom`), or null when it takes none: a boolean, a number, a length, a slot.
 * A union has the constants of its members, so `ec` has two and `size` has one and an open part.
 */
export function shapeOf(schema, constants, typeName) {
    if (!typeName) return null;
    const byType = new Map(constants.map((c) => [c.type, c]));
    const members = schema.unionMembers(typeName) ?? [typeName];
    const named = members.filter((t) => byType.has(t)).map((t) => byType.get(t).name);
    if (!named.length) return null;
    return { constants: named, open: members.filter((t) => !byType.has(t)) };
}

/** The TypeScript type of an open part of a type: the part that has one, `string` for the rest. */
export const OPEN_TYPESCRIPT = { PageSizeCustom: 'PageSizeCustom' };

/** The TypeScript type of an attribute whose type has `shape`: its constants' types, or `string` when an open part has no type. */
export function typeScriptOf(shape) {
    const open = shape.open.map((type) => OPEN_TYPESCRIPT[type] ?? 'string');
    if (open.includes('string')) return 'string';
    return [...shape.constants, ...open].join(' | ');
}

/**
 * What the stamp of an existing PDF takes that the schema has no element for: its `anchor` is the schema's `Anchor`, its
 * `align` and `justify` the schema's `Align`, and its `scale` is these two words, which the core reads in `stamp.rs`.
 */
export const STAMP_CONSTANTS = [{
    name: 'StampScale', type: null, area: 'engine', values: ['fit', 'fill'],
    doc: 'The values of the `scale` option of a stamp: how its image is sized in its box, keeping its proportions.',
}].map((c) => ({ ...c, members: c.values.map((value) => ({ value, ...memberNames(c.name, value) })) }));

/** The types of a stamp's options in TypeScript, which `StampAttr` is written with by hand. */
export const STAMP_TYPESCRIPT = { anchor: 'Anchor', align: 'Align', justify: 'Align', scale: 'StampScale' };
