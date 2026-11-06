// The check for the JS SDK (`@lpdfio/lpdf`), run in this process.
//
// The code the generator writes is a program: an import of what it uses, the engine, the document as an expression
// and the render. The expression is run as a function of the imported names, and the engine here renders both it and
// the XML.

import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';

import { declaredAssets } from './assets.mjs';
import { firstLine } from './common.mjs';

export const target = 'js';

const PROGRAM = /^import \{([^}]*)\} from '[^']*'\n[\s\S]*?\nconst doc = ([\s\S]*?)\n\nconst pdf = [^\n]*\n?$/;

/** The names a JS program imports, and the expression that builds its document; or why it is not a program. */
export function shape(code) {
    const match = PROGRAM.exec(code);
    if (!match) throw new Error('the program is not an import, the engine, `const doc = …` and `const pdf = …`');
    return { names: match[1].split(',').map((name) => name.trim()).filter(Boolean), expression: match[2] };
}

/** The function that builds the document of a JS program, from what the SDK exports. */
function builder(code, sdk) {
    const { names, expression } = shape(code);
    const build = new Function(...names, `return ${expression};`);
    return () => build(...names.map((name) => sdk[name]));
}

export async function run({ sdk: folder, base, placeholders, documents, createdOn }) {
    const sdk = createRequire(import.meta.url)(path.resolve(folder));
    const results = new Map();
    // No passwords: a document that asks for encryption renders unencrypted, as a preview does.
    const options = { skipEncryption: true, createdOn };
    const bytes = new Map();
    const load = (file) => bytes.get(file) ?? bytes.set(file, readFileSync(file)).get(file);

    for (const { index, xml, code } of documents) {
        let build;
        try {
            build = builder(code, sdk);
        } catch (e) {
            results.set(index, { status: 'shape', message: firstLine(e.message) });
            continue;
        }
        const engine = sdk.L.engine();
        for (const { kind, key, path: file } of declaredAssets(xml, base, placeholders)) {
            const method = { font: 'loadFont', image: 'loadImage', file: 'loadFile', profile: 'loadProfile' }[kind];
            engine[method](key, load(file));
        }
        let fromXml;
        try {
            fromXml = await engine.render(xml, options);
        } catch (e) {
            results.set(index, { status: 'xml', message: firstLine(e.message) });
            continue;
        }
        try {
            const fromCode = await engine.render(build(), options);
            results.set(index, Buffer.compare(Buffer.from(fromXml), Buffer.from(fromCode)) === 0 ? { status: 'ok' } : { status: 'differs' });
        } catch (e) {
            results.set(index, { status: 'code', message: firstLine(e.message) });
        }
    }
    return results;
}
