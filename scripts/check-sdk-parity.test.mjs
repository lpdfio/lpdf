// Tests of what check-sdk-parity.mjs checks about the constants (scripts/lib/parity.mjs): that an entry point exports
// them all, and that the TypeScript classes written by hand have the constants' types. Run: node --test scripts/check-sdk-parity.test.mjs

import assert from 'node:assert/strict';
import path from 'node:path';
import { describe, it } from 'node:test';
import { fileURLToPath } from 'node:url';

import { constantsOf, STAMP_CONSTANTS } from './lib/constants.mjs';
import { exportProblems, interfaceTypes, stampTypeProblems, TYPED, typeProblems } from './lib/parity.mjs';
import { loadSchema } from './lib/xsd.mjs';

const schema = loadSchema(path.join(path.dirname(fileURLToPath(import.meta.url)), '../schema/lpdf.xsd'));
const constants = constantsOf(schema);
const names = [...constants, ...STAMP_CONSTANTS].map((c) => c.name);
const camelToKebab = (name) => name.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);

describe('the exports of an entry point', () => {
    it('are all of them with one star export, in TypeScript and in Python', () => {
        assert.deepEqual(exportProblems('src/index.ts', "export * from './constants';\n", names), []);
        assert.deepEqual(exportProblems('src/lpdf/__init__.py', 'from .constants import *  # noqa: F403\n', names), []);
    });

    it('are all of them when each is named', () => {
        assert.deepEqual(exportProblems('src/index.ts', `export { ${names.join(', ')} } from './constants';`, names), []);
    });

    it('lack the ones that no export names', () => {
        const some = names.filter((name) => name !== 'Ocg' && name !== 'StampScale');
        assert.deepEqual(exportProblems('src/browser.ts', `export { ${some.join(', ')} } from './constants';`, names), [
            "src/browser.ts doesn't export Ocg", "src/browser.ts doesn't export StampScale",
        ]);
        assert.equal(exportProblems('src/lpdf/__init__.py', 'from .kit.orientation import Orientation\n', names).length, names.length - 1);
    });

    it('do not count the name of a constant inside a longer name', () => {
        assert.deepEqual(exportProblems('src/index.ts', 'export { PdfALevelX } from "./constants";', ['PdfALevel']), ["src/index.ts doesn't export PdfALevel"]);
    });
});

describe('the TypeScript interfaces written by hand', () => {
    const source = `
export interface SectionAttr {
  size?:        PageSize | PageSizeCustom;
  orientation?: string;
  /** A comment: with a colon. */
  pageStart?: number;
}

export interface Other {
  orientation?: string;
}
`;

    it('are read by the names of the classes asked for, with the schema\'s attribute names', () => {
        assert.deepEqual(interfaceTypes(source, ['SectionAttr'], camelToKebab), {
            SectionAttr: { size: 'PageSize | PageSizeCustom', orientation: 'string', 'page-start': 'number' },
        });
    });

    it('have the constants\' types when the schema says so', () => {
        const actual = { size: 'PageSize | PageSizeCustom', orientation: 'Orientation', type: 'SectionType', 'page-format': 'ListFormat' };
        assert.deepEqual(typeProblems(schema, constants, 'SectionAttr', TYPED.SectionAttr, actual), []);
    });

    it('are reported when an attribute is a string where the schema gives a constant', () => {
        const actual = { size: 'string', orientation: 'string' };
        assert.deepEqual(typeProblems(schema, constants, 'SectionAttr', TYPED.SectionAttr, actual), [
            "node: SectionAttr.size is string, and the schema's PageSize makes it PageSize | PageSizeCustom",
            "node: SectionAttr.orientation is string, and the schema's Orientation makes it Orientation",
        ]);
    });

    it('leave an attribute alone that takes no constant, or that the class does not have', () => {
        assert.deepEqual(typeProblems(schema, constants, 'DocumentOutput', TYPED.DocumentOutput, { allow: 'string', linearize: 'boolean' }), []);
        assert.deepEqual(typeProblems(schema, constants, 'SectionAttr', TYPED.SectionAttr, {}), []);
    });

    it('have a stamp whose options take the constants of the stamp', () => {
        assert.deepEqual(stampTypeProblems({ anchor: 'Anchor', align: 'Align', justify: 'Align', scale: 'StampScale' }), []);
        assert.deepEqual(stampTypeProblems({ anchor: 'string', align: 'Align', justify: 'Align', scale: 'StampScale' }), [
            'node: StampAttr.anchor is string, and should be Anchor',
        ]);
    });
});
