// The check for the Python SDK (`lpdf`): one Python program that runs every document.
//
// The code the generator writes is a script: the import, the engine, the document as `doc = …` and the render. Each
// document becomes a module of its own without the render, and the program imports it and reads its `doc`, then renders
// the document and the XML through one engine. The SDK is the `lpdf` package under `src` of the repository, or in the
// image's /app, with the engine file the package finds beside it.

import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';

import { firstLine, missingResults, readResults, writeDocuments } from './common.mjs';
import { SDK_IN_DOCKER, inDocker } from './docker.mjs';

export const target = 'python';

const PROGRAM = /^([\s\S]*?\ndoc = [\s\S]*?)\n\npdf = engine\.render\(doc\)\n?$/;

/** The module of one document: its code without the render; or why it is not a program. */
export function shape(code) {
    const match = PROGRAM.exec(code);
    if (!match) throw new Error('the program is not the import, the engine, `doc = …` and `pdf = …`');
    return `${match[1]}\n`;
}

const CHECK = (packages, createdOn) => `import importlib.util
import sys
from pathlib import Path

sys.path.insert(0, '${packages}')

from lpdf import L, RenderOptions


def first(message):
    for line in message.splitlines():
        if line.strip():
            return line.strip()
    return ''


def check(work, index):
    engine = L.engine()
    for line in Path(f'{work}/docs/{index}.assets').read_text(encoding='utf-8').splitlines():
        kind, key, path = line.split('\\t')
        data = Path(path).read_bytes()
        {'font': engine.load_font, 'image': engine.load_image, 'file': engine.load_file, 'profile': engine.load_profile}[kind](key, data)
    options = RenderOptions(created_on='${createdOn}', skip_encryption=True)
    try:
        from_xml = engine.render(Path(f'{work}/docs/{index}.xml').read_text(encoding='utf-8'), options)
    except Exception as e:
        return 'xml', first(str(e))
    try:
        spec = importlib.util.spec_from_file_location(f'doc{index}', f'{work}/code/{index}.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        from_code = engine.render(module.doc, options)
        return ('ok' if from_xml == from_code else 'differs'), ''
    except Exception as e:
        return 'code', first(f'{type(e).__name__}: {e}')


work = sys.argv[1]
for index in Path(f'{work}/indexes.txt').read_text(encoding='utf-8').split():
    status, message = check(work, index)
    sys.stdout.write(f'RESULT\\t{index}\\t{status}\\t' + message.replace('\\t', ' ').replace('\\n', ' ') + '\\n')
    sys.stdout.flush()
`;

export async function run({ sdk, work, base, placeholders, documents, createdOn, docker }) {
    const results = new Map();
    mkdirSync(path.join(work, 'code'), { recursive: true });
    writeDocuments(work, documents, base, placeholders);

    const indexes = [];
    for (const { index, code } of documents) {
        try {
            writeFileSync(path.join(work, 'code', `${index}.py`), shape(code));
            indexes.push(index);
        } catch (e) {
            results.set(index, { status: 'shape', message: firstLine(e.message) });
        }
    }
    if (indexes.length === 0) return results;

    writeFileSync(path.join(work, 'check.py'), CHECK(docker ? SDK_IN_DOCKER : path.resolve(sdk, 'src'), createdOn));
    writeFileSync(path.join(work, 'indexes.txt'), indexes.join('\n') + '\n');
    const ran = inDocker(docker, 'python3', [path.join(work, 'check.py'), work], { work, base });
    for (const [index, result] of readResults(ran.stdout)) results.set(index, result);
    return missingResults(results, documents, firstLine(ran.stderr) || ran.stdout);
}
