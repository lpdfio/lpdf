// Tests of the constants the SDKs name for the schema's enumerations (scripts/lib/constants.mjs), which
// gen-sdk-api.mjs writes and check-sdk-parity.mjs checks. Run: node --test scripts/constants.test.mjs

import assert from 'node:assert/strict';
import path from 'node:path';
import { describe, it } from 'node:test';
import { fileURLToPath } from 'node:url';

import { constantsOf, EXCLUDED, memberNames, shapeOf, STAMP_CONSTANTS, typeScriptOf } from './lib/constants.mjs';
import { loadSchema } from './lib/xsd.mjs';

const schema = loadSchema(path.join(path.dirname(fileURLToPath(import.meta.url)), '../schema/lpdf.xsd'));
const constants = constantsOf(schema);
const named = (name) => constants.find((c) => c.name === name);

describe('the members of a constant', () => {
    it('are the value in each language\'s case', () => {
        assert.deepEqual(memberNames('Orientation', 'landscape'), { pascal: 'Landscape', snake: 'LANDSCAPE' });
        assert.deepEqual(memberNames('Paginate', 'break-before'), { pascal: 'BreakBefore', snake: 'BREAK_BEFORE' });
        assert.deepEqual(memberNames('Grow', 'width height'), { pascal: 'WidthHeight', snake: 'WIDTH_HEIGHT' });
        assert.deepEqual(memberNames('BarcodeType', 'gs1-128'), { pascal: 'Gs1128', snake: 'GS1_128' });
    });

    it('take the last word of the constant\'s name before a digit, which no language accepts first', () => {
        assert.deepEqual(memberNames('PdfALevel', '2a'), { pascal: 'Level2a', snake: 'LEVEL_2A' });
        assert.deepEqual(memberNames('PdfUaPart', '1'), { pascal: 'Part1', snake: 'PART_1' });
        assert.deepEqual(memberNames('PdfXVersion', '4'), { pascal: 'Version4', snake: 'VERSION_4' });
        assert.deepEqual(memberNames('Pdf417EcLevel', '0'), { pascal: 'Level0', snake: 'LEVEL_0' });
    });

    it('start with a letter in every constant of the schema', () => {
        for (const { name, members } of [...constants, ...STAMP_CONSTANTS]) {
            for (const { pascal, snake } of members) {
                assert.match(pascal, /^[A-Z][A-Za-z0-9]*$/, `${name}.${pascal}`);
                assert.match(snake, /^[A-Z][A-Z0-9_]*$/, `${name}.${snake}`);
            }
        }
    });
});

describe('the constants of the schema', () => {
    it('are one for each closed enumeration but the ones left out', () => {
        const types = schema.enumerations().map((e) => e.type).filter((type) => !(type in EXCLUDED));
        assert.equal(constants.length, types.length);
        assert.ok(constants.length >= 39, `${constants.length} constants`);
        for (const type of Object.keys(EXCLUDED)) assert.ok(schema.enumerations().some((e) => e.type === type), `${type} is not an enumeration of the schema`);
        assert.equal(named('TokenScale'), undefined);
    });

    it('name the named part of a union after the union', () => {
        assert.deepEqual(named('PageSize').values, ['a4', 'a3', 'a5', 'letter', 'legal']);
        assert.deepEqual(named('PageScope').values, ['each', 'first', 'last', 'odd', 'even']);
        assert.equal(named('PageSizeNamed'), undefined);
    });

    it('give the error correction of a barcode two constants, since its levels depend on its type', () => {
        assert.deepEqual(named('QrEcLevel').values, ['L', 'M', 'Q', 'H']);
        assert.equal(named('Pdf417EcLevel').values.length, 9);
        assert.deepEqual(shapeOf(schema, constants, 'EcLevel'), { constants: ['QrEcLevel', 'Pdf417EcLevel'], open: [] });
    });

    it('are refused for an enumeration with no documentation, which is its doc comment', () => {
        const without = { enumerations: () => [{ type: 'Fresh', values: ['a', 'b'], doc: '' }] };
        assert.throws(() => constantsOf(without), /Fresh.*needs an xs:documentation/);
    });

    it('are refused for an enumeration written inline on an attribute, which has no name to make a constant of', () => {
        const inline = { inlineEnumerations: () => ['divider.zzz'], enumerations: () => [] };
        assert.throws(() => constantsOf(inline), /divider\.zzz declares an enumeration inline: give it a named xs:simpleType/);
    });

    it('are made from a schema with no enumeration inline', () => {
        assert.deepEqual(schema.inlineEnumerations(), []);
    });

    it('are refused when two members would have the same name', () => {
        const twice = { enumerations: () => [{ type: 'Twice', values: ['a-b', 'a b'], doc: 'Two words.' }] };
        assert.throws(() => constantsOf(twice), /two members called AB/);
    });
});

describe('the type of an attribute in TypeScript', () => {
    it('is the constant\'s, or the union of the constants an attribute takes', () => {
        assert.equal(typeScriptOf(shapeOf(schema, constants, 'Orientation')), 'Orientation');
        assert.equal(typeScriptOf(shapeOf(schema, constants, 'EcLevel')), 'QrEcLevel | Pdf417EcLevel');
    });

    it('has the open part of a size, a width and a height', () => {
        assert.equal(typeScriptOf(shapeOf(schema, constants, 'PageSize')), 'PageSize | PageSizeCustom');
    });

    it('is a string where the open part is a range or a length, and a type with no constant has none', () => {
        assert.equal(typeScriptOf(shapeOf(schema, constants, 'PageScope')), 'string');
        assert.equal(shapeOf(schema, constants, 'TokenScaleOrPt'), null);
        assert.equal(shapeOf(schema, constants, 'Boolean'), null);
        assert.equal(shapeOf(schema, constants, 'Permissions'), null);
    });

    it('is given for every attribute that takes the values of a closed enumeration', () => {
        const left = new Set(Object.keys(EXCLUDED));
        for (const element of schema.allElements()) {
            for (const a of element.attributes) {
                if (!a.typeName) continue;
                const enumeration = schema.enumerations().find((e) => e.type === a.typeName);
                if (!enumeration || left.has(a.typeName)) continue;
                assert.ok(shapeOf(schema, constants, a.typeName), `${element.name}@${a.name} takes ${a.typeName}, which has no constant`);
            }
        }
    });
});
