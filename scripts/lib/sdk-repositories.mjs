// Where the four SDKs are, for the scripts that read or write their source.
//
// By default each SDK is the repository checked out under src/sdk, and all four are used. An SDK repository's
// own CI has only itself, next to a clone of this repository for the schema and the scripts, so it says where
// it is: `--sdk node=.` (repeatable). Then only the SDKs named are used.

import { existsSync } from 'node:fs';
import path from 'node:path';

export const LANGUAGES = ['dotnet', 'node', 'python', 'php'];

function fail(message) {
    console.error(message);
    process.exit(2);
}

/**
 * Reads `--sdk <language>=<folder>` (or `--sdk=<language>=<folder>`) from `argv`.
 * Returns the repository folder of every language and the languages to use.
 */
export function sdkRepositories(root, argv) {
    const given = {};
    for (let i = 0; i < argv.length; i++) {
        let value;
        if (argv[i] === '--sdk') value = argv[++i] ?? '';
        else if (argv[i].startsWith('--sdk=')) value = argv[i].slice('--sdk='.length);
        else continue;

        const at = value.indexOf('=');
        const language = value.slice(0, at);
        const folder = value.slice(at + 1);
        if (at < 1 || !folder || !LANGUAGES.includes(language)) {
            fail(`--sdk expects <language>=<folder>, with the language one of ${LANGUAGES.join(', ')}; got '${value}'`);
        }
        given[language] = path.resolve(folder);
    }

    const selected = Object.keys(given).length ? LANGUAGES.filter((language) => language in given) : LANGUAGES;
    const repositories = Object.fromEntries(LANGUAGES.map((language) => [language, given[language] ?? path.join(root, 'src/sdk', language)]));
    for (const language of selected) {
        if (!existsSync(repositories[language])) fail(`the ${language} SDK is not at ${repositories[language]}`);
    }
    return { repositories, selected };
}
