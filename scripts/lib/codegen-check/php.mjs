// The check for the PHP SDK (`lpdf/lpdf`): one PHP program that runs every document.
//
// The code the generator writes is a script: the autoloader, the `use` lines, the engine, the document as `$doc = …;`
// and the render. Each document becomes a file of its own that ends by returning the document instead of rendering it,
// and the program includes it, then renders the document and the XML through one engine. The SDK is the folder with
// `vendor/autoload.php` (`composer install`), and the engine it picks is the one it picks for any script: the native
// library through FFI, else the `wasmtime` program.

import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';

import { firstLine, missingResults, readResults, writeDocuments } from './common.mjs';
import { SDK_IN_DOCKER, inDocker } from './docker.mjs';

export const target = 'php';

const PROGRAM = /^<\?php\n\nrequire_once 'vendor\/autoload\.php';\n\n([\s\S]*?)\n\n\$pdf = \$engine->render\(\$doc\);\n?$/;

/** The file of one document: its code, ending by returning the document instead of rendering it; or why it is not a program. */
export function shape(code) {
    const match = PROGRAM.exec(code);
    if (!match) throw new Error('the program is not the autoloader, the use lines, the engine, `$doc = …;` and `$pdf = …;`');
    return `<?php\n\n${match[1]}\n\nreturn $doc;\n`;
}

const CHECK = (autoload, createdOn) => `<?php
declare(strict_types=1);

require '${autoload}';

use Lpdf\\L;
use Lpdf\\Engine\\RenderOptions;

function first(string $message): string
{
    foreach (explode("\\n", $message) as $line) {
        if (trim($line) !== '') {
            return trim($line);
        }
    }
    return '';
}

function check(string $work, string $index): array
{
    $engine = L::engine();
    foreach (file("$work/docs/$index.assets", FILE_IGNORE_NEW_LINES | FILE_SKIP_EMPTY_LINES) as $line) {
        [$kind, $key, $path] = explode("\\t", $line);
        $bytes = file_get_contents($path);
        match ($kind) {
            'font' => $engine->loadFont($key, $bytes),
            'image' => $engine->loadImage($key, $bytes),
            'file' => $engine->loadFile($key, $bytes),
            'profile' => $engine->loadProfile($key, $bytes),
        };
    }
    $options = new RenderOptions(createdOn: '${createdOn}', skipEncryption: true);
    try {
        $fromXml = $engine->render(file_get_contents("$work/docs/$index.xml"), $options);
    } catch (\\Throwable $e) {
        return ['xml', first($e->getMessage())];
    }
    try {
        // In a function of its own: the code makes an engine of its own, which must not replace the one loaded here.
        $document = (static fn (string $file) => require $file)("$work/code/$index.php");
        $fromCode = $engine->render($document, $options);
        return [$fromXml === $fromCode ? 'ok' : 'differs', ''];
    } catch (\\Throwable $e) {
        return ['code', first($e->getMessage())];
    }
}

$work = $argv[1];
foreach (file("$work/indexes.txt", FILE_IGNORE_NEW_LINES | FILE_SKIP_EMPTY_LINES) as $index) {
    [$status, $message] = check($work, $index);
    echo "RESULT\\t$index\\t$status\\t" . str_replace(["\\t", "\\n"], ' ', $message) . "\\n";
}
`;

export async function run({ sdk, work, base, placeholders, documents, createdOn, docker }) {
    const results = new Map();
    mkdirSync(path.join(work, 'code'), { recursive: true });
    writeDocuments(work, documents, base, placeholders);

    const indexes = [];
    for (const { index, code } of documents) {
        try {
            writeFileSync(path.join(work, 'code', `${index}.php`), shape(code));
            indexes.push(index);
        } catch (e) {
            results.set(index, { status: 'shape', message: firstLine(e.message) });
        }
    }
    if (indexes.length === 0) return results;

    writeFileSync(path.join(work, 'check.php'), CHECK(path.join(docker ? SDK_IN_DOCKER : path.resolve(sdk), 'vendor/autoload.php'), createdOn));
    writeFileSync(path.join(work, 'indexes.txt'), indexes.join('\n') + '\n');
    const ran = inDocker(docker, 'php', [path.join(work, 'check.php'), work], { work, base });
    for (const [index, result] of readResults(ran.stdout)) results.set(index, result);
    return missingResults(results, documents, firstLine(ran.stderr) || ran.stdout);
}
