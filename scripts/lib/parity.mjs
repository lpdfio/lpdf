// The checks of check-sdk-parity.mjs that read the constants, as functions of text, so that they can be tested without
// an SDK: what an entry point exports, and the TypeScript types of the attributes that are written by hand.

import { shapeOf, STAMP_TYPESCRIPT, typeScriptOf } from './constants.mjs';

/** The hand-written TypeScript classes whose attributes take the constants' types, with the schema element of each. */
export const TYPED = { DocumentAttr: 'document', SectionAttr: 'section', DocumentOutput: 'output' };

/**
 * What an entry point's `text` doesn't export of `names`, as one line each. `export * from './constants'` in
 * TypeScript and `from .constants import *` in Python export all of them; without one, each has to be named.
 */
export function exportProblems(entry, text, names) {
    if (/export \* from '\.\/constants'/.test(text) || /from \.constants import \*/.test(text)) return [];
    return names.filter((name) => !new RegExp(`\\b${name}\\b`).test(text)).map((name) => `${entry} doesn't export ${name}`);
}

/**
 * The attributes of the interfaces named in `classes`, in the TypeScript `text`: `{ Class: { attribute: type } }`, each
 * attribute's name turned from camel case into the schema's by `toSchemaName`.
 */
export function interfaceTypes(text, classes, toSchemaName) {
    const found = {};
    for (const [, name, body] of text.matchAll(/export interface (\w+)(?:\s+extends[^{]+)?\s*\{([\s\S]*?)\n\}/g)) {
        if (!classes.includes(name)) continue;
        found[name] = Object.fromEntries([...body.matchAll(/^\s+(\w+)\??\s*:\s*([^;]+);/gm)].map((m) => [toSchemaName(m[1]), m[2].trim()]));
    }
    return found;
}

/** The attributes of the hand-written class `name` (of the schema's `element`) whose type is not the constants' type. */
export function typeProblems(schema, constants, name, element, actual) {
    const problems = [];
    for (const a of schema.element(element).attributes) {
        const shape = shapeOf(schema, constants, a.typeName);
        if (!shape || !(a.name in actual)) continue;
        const wanted = typeScriptOf(shape);
        if (actual[a.name].replace(/\s+/g, ' ') !== wanted) {
            problems.push(`node: ${name}.${a.name} is ${actual[a.name]}, and the schema's ${a.typeName} makes it ${wanted}`);
        }
    }
    return problems;
}

/** The options of a stamp whose type is not the one its constant gives. */
export function stampTypeProblems(actual) {
    return Object.entries(STAMP_TYPESCRIPT)
        .filter(([option, wanted]) => actual[option] !== wanted)
        .map(([option, wanted]) => `node: StampAttr.${option} is ${actual[option]}, and should be ${wanted}`);
}
