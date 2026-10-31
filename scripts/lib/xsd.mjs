// Reads lpdf.xsd into the element and attribute lists the SDK generator and checker work from.
//
// The schema is plain XSD without entities or CDATA, so a small tokenizer is enough: elements and their
// attributes are kept, text is dropped except inside xs:documentation.

import { readFileSync } from 'node:fs';

/** Parses XML text into { name, attrs, children, text } nodes. */
export function parseXml(text) {
    const root = { name: '#root', attrs: {}, children: [], text: '' };
    const stack = [root];
    const token = /<!--[\s\S]*?-->|<\?[\s\S]*?\?>|<(\/?)([\w:.-]+)((?:\s+[\w:.-]+\s*=\s*"[^"]*")*)\s*(\/?)>|([^<]+)/g;
    for (const m of text.matchAll(token)) {
        const [, closing, name, attrText, selfClosing, chars] = m;
        if (chars !== undefined) {
            stack[stack.length - 1].text += chars;
        } else if (name === undefined) {
            continue;
        } else if (closing) {
            stack.pop();
        } else {
            const attrs = {};
            for (const a of (attrText ?? '').matchAll(/([\w:.-]+)\s*=\s*"([^"]*)"/g)) attrs[a[1]] = a[2];
            const node = { name, attrs, children: [], text: '' };
            stack[stack.length - 1].children.push(node);
            if (!selfClosing) stack.push(node);
        }
    }
    return root.children[0];
}

const local = (name) => name.replace(/^xs:/, '');

// The built-in types the schema uses for numbers, with the least value each allows.
const BUILT_IN = {
    'xs:integer': { kind: 'integer' },
    'xs:positiveInteger': { kind: 'integer', min: 1 },
    'xs:nonNegativeInteger': { kind: 'integer', min: 0 },
    'xs:decimal': { kind: 'decimal' },
};

function descendants(node, name, found = []) {
    for (const child of node.children) {
        if (local(child.name) === name) found.push(child);
        descendants(child, name, found);
    }
    return found;
}

function documentation(node) {
    const annotation = node.children.find((c) => local(c.name) === 'annotation');
    const doc = annotation?.children.find((c) => local(c.name) === 'documentation');
    return doc ? doc.text.replace(/\s+/g, ' ').trim() : '';
}

/**
 * Loads the schema. `element(name, where)` finds an element declaration: `where` is 'global' for a
 * top-level element, 'primitives' for a drawing primitive in the CanvasPrimitives group, or an element
 * name for one declared inside that element (`region` inside `layout`).
 */
export function loadSchema(path) {
    const schema = parseXml(readFileSync(path, 'utf8'));
    const groups = new Map(
        schema.children.filter((c) => local(c.name) === 'attributeGroup').map((g) => [g.attrs.name, g]),
    );

    function attributesOf(node, seen = new Set()) {
        const found = [];
        for (const child of node.children) {
            const tag = local(child.name);
            if (tag === 'attribute' && child.attrs.name) {
                found.push({
                    name: child.attrs.name,
                    required: child.attrs.use === 'required',
                    doc: documentation(child),
                    type: attributeType(child),
                });
            } else if (tag === 'attributeGroup' && child.attrs.ref && !seen.has(child.attrs.ref)) {
                const group = groups.get(child.attrs.ref);
                if (group) found.push(...attributesOf(group, new Set([...seen, child.attrs.ref])));
            }
        }
        return found;
    }

    function describe(element) {
        // A token row names its type, which the other rows share; every other element declares its own.
        const complexType = element.children.find((c) => local(c.name) === 'complexType')
            ?? complexTypes.get(element.attrs.type);
        const byName = new Map();
        for (const a of complexType ? attributesOf(complexType) : []) if (!byName.has(a.name)) byName.set(a.name, a);
        return { name: element.attrs.name, doc: documentation(element), attributes: [...byName.values()] };
    }

    const globals = schema.children.filter((c) => local(c.name) === 'element' && c.attrs.name);

    const simpleTypes = new Map(
        schema.children.filter((c) => local(c.name) === 'simpleType' && c.attrs.name).map((s) => [s.attrs.name, s]),
    );
    const complexTypes = new Map(
        schema.children.filter((c) => local(c.name) === 'complexType' && c.attrs.name).map((c) => [c.attrs.name, c]),
    );
    const modelGroups = new Map(
        schema.children.filter((c) => local(c.name) === 'group' && c.attrs.name).map((g) => [g.attrs.name, g]),
    );
    const enumerationOf = (node) => descendants(node, 'enumeration').map((e) => e.attrs.value);

    /** The type of a named or built-in type: see attributeType. */
    function namedType(name) {
        if (name in BUILT_IN) return { ...BUILT_IN[name] };
        if (name.startsWith('xs:')) return { kind: 'string' };
        const simple = simpleTypes.get(name);
        if (!simple) throw new Error(`schema has no simple type '${name}'`);
        return simpleType(simple);
    }

    function simpleType(node) {
        const inner = node.children.find((c) => ['restriction', 'union', 'list'].includes(local(c.name)));
        if (!inner || local(inner.name) === 'list') return { kind: 'string' };

        if (local(inner.name) === 'union') {
            const members = [
                ...(inner.attrs.memberTypes ?? '').split(/\s+/).filter(Boolean).map(namedType),
                ...inner.children.filter((c) => local(c.name) === 'simpleType').map(simpleType),
            ];
            const numbers = members.filter((m) => m.kind === 'integer' || m.kind === 'decimal');
            if (numbers.length === 0 && members.every((m) => m.kind === 'string')) return { kind: 'string' };
            if (numbers.length === 1 && members.length > 1 && members.every((m) => m === numbers[0] || m.kind === 'string')) {
                return { ...numbers[0], kind: 'name-or-number', number: numbers[0].kind };
            }
            throw new Error(`a union of ${members.map((m) => m.kind).join(', ')} has no type in the SDKs`);
        }

        const values = enumerationOf(inner);
        if (values.length === 2 && values.includes('true') && values.includes('false')) return { kind: 'boolean' };
        const nested = inner.children.find((c) => local(c.name) === 'simpleType');
        const base = nested ? simpleType(nested) : namedType(inner.attrs.base ?? 'xs:string');
        if (base.kind !== 'integer' && base.kind !== 'decimal') return base;
        const facet = (name) => {
            const found = inner.children.find((c) => local(c.name) === name);
            return found === undefined ? undefined : Number(found.attrs.value);
        };
        const type = { ...base };
        const minInclusive = facet('minInclusive');
        const minExclusive = facet('minExclusive');
        const maxInclusive = facet('maxInclusive');
        if (minInclusive !== undefined && (type.min === undefined || minInclusive > type.min)) type.min = minInclusive;
        if (minExclusive !== undefined) { type.min = minExclusive; type.minExclusive = true; }
        if (maxInclusive !== undefined) type.max = maxInclusive;
        return type;
    }

    /**
     * The type of an attribute, as the SDKs and the engine use it: `{ kind }`, where kind is `boolean`, `integer`,
     * `decimal`, `name-or-number` (a name, or a number of the kind `number`: line-height) or `string`. A number
     * has the bounds the schema gives it, `min` and `max`, with `minExclusive` when its least value is left out.
     */
    function attributeType(attribute) {
        if (attribute.attrs.type) return namedType(attribute.attrs.type);
        const inline = attribute.children.find((c) => local(c.name) === 'simpleType');
        return inline ? simpleType(inline) : { kind: 'string' };
    }

    /** Every element declaration, the local ones with the name of the element they are declared in as `within`. */
    function allElements() {
        const found = [];
        const walk = (node, within) => {
            for (const child of node.children) {
                if (local(child.name) === 'element' && child.attrs.name) {
                    found.push({ ...describe(child), within });
                    walk(child, child.attrs.name);
                } else {
                    walk(child, within);
                }
            }
        };
        for (const element of globals) {
            found.push({ ...describe(element), within: null });
            walk(element, element.attrs.name);
        }
        return found;
    }

    return {
        groups,
        allElements,
        /** The names of the elements a model group such as `CanvasPrimitives` holds. */
        groupElements(name) {
            const group = modelGroups.get(name);
            if (!group) throw new Error(`schema has no group '${name}'`);
            return descendants(group, 'element').map((e) => e.attrs.ref ?? e.attrs.name);
        },
        /** The values of a named simple type, such as `Orientation`. */
        simpleTypeValues(name) {
            const type = simpleTypes.get(name);
            if (!type) throw new Error(`schema has no simple type '${name}'`);
            return enumerationOf(type);
        },
        /** The values an attribute is restricted to, such as `type` on `field`. */
        attributeValues(elementName, attributeName) {
            const element = globals.find((e) => e.attrs.name === elementName);
            const attribute = element && descendants(element, 'attribute').find((a) => a.attrs.name === attributeName);
            if (!attribute) throw new Error(`schema has no attribute '${attributeName}' on '${elementName}'`);
            return enumerationOf(attribute);
        },
        element(name, where = 'global') {
            let found;
            if (where === 'global') found = globals.find((e) => e.attrs.name === name);
            else {
                const outer = globals.find((e) => e.attrs.name === where);
                found = outer && descendants(outer, 'element').find((e) => e.attrs.name === name && e.children.length);
            }
            if (!found) throw new Error(`schema has no element '${name}' (${where})`);
            return describe(found);
        },
    };
}
