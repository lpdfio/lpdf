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
 * name for one declared inside that element (`region` inside `layout`, `layer` inside `canvas`).
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
                });
            } else if (tag === 'attributeGroup' && child.attrs.ref && !seen.has(child.attrs.ref)) {
                const group = groups.get(child.attrs.ref);
                if (group) found.push(...attributesOf(group, new Set([...seen, child.attrs.ref])));
            }
        }
        return found;
    }

    function describe(element) {
        const complexType = element.children.find((c) => local(c.name) === 'complexType');
        const byName = new Map();
        for (const a of complexType ? attributesOf(complexType) : []) if (!byName.has(a.name)) byName.set(a.name, a);
        return { name: element.attrs.name, doc: documentation(element), attributes: [...byName.values()] };
    }

    const globals = schema.children.filter((c) => local(c.name) === 'element' && c.attrs.name);
    const canvasGroup = schema.children.find((c) => local(c.name) === 'group' && c.attrs.name === 'CanvasPrimitives');

    const simpleTypes = new Map(
        schema.children.filter((c) => local(c.name) === 'simpleType' && c.attrs.name).map((s) => [s.attrs.name, s]),
    );
    const enumerationOf = (node) => descendants(node, 'enumeration').map((e) => e.attrs.value);

    return {
        groups,
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
            else if (where === 'primitives') found = descendants(canvasGroup, 'element').find((e) => e.attrs.name === name);
            else {
                const outer = globals.find((e) => e.attrs.name === where);
                found = outer && descendants(outer, 'element').find((e) => e.attrs.name === name && e.children.length);
            }
            if (!found) throw new Error(`schema has no element '${name}' (${where})`);
            return describe(found);
        },
    };
}
