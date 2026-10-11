#!/usr/bin/env node
// Downloads the extension a core release candidate built, the .vsix that
// lpdf-vscode's rc.yml keeps as a run artifact, for the manual check before a
// release. Prints how to install it and what to check.
//
//   make rc-vsix                   the newest RC that lpdf-vscode built
//   make rc-vsix RC=v0.25.0-rc.2   a specific one
//
// The file lands in src/vscode/.rc/<tag>/, which lpdf-vscode's *.vsix rule
// keeps out of git. Needs gh signed in: GitHub serves artifacts only with a
// token, even from a public repo.

import { execFileSync } from 'node:child_process';
import { rmSync } from 'node:fs';
import { join } from 'node:path';

const REPO = 'lpdfio/lpdf-vscode';
// rc.yml names each run "RC <tag>: …", which is how a run is matched to its RC.
const RUN_TITLE_RE = /^RC (v\d+\.\d+\.0-rc\.\d+):/;
const DOWNLOADS = join(import.meta.dirname, '..', 'src', 'vscode', '.rc');

function fail(message)
{
    console.error(`rc-vsix: ${message}`);
    process.exit(1);
}

// gh prints its own error, so a failure only needs the exit code.
function gh(args)
{
    try
    {
        return execFileSync('gh', args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'] });
    }
    catch
    {
        fail(`gh ${args.slice(0, 2).join(' ')} failed`);
    }
}

const wanted = (process.argv[2] || '').trim();
const runs = JSON.parse(gh(['run', 'list', '--repo', REPO, '--workflow', 'rc.yml', '--limit', '30', '--json', 'databaseId,displayTitle']));
const run = runs.find(r => (wanted ? r.displayTitle.startsWith(`RC ${wanted}:`) : RUN_TITLE_RE.test(r.displayTitle)));
if (!run) fail(wanted ? `no rc.yml run in ${REPO} for ${wanted}` : `no rc.yml run in ${REPO} yet`);
const tag = run.displayTitle.match(RUN_TITLE_RE)[1];

const folder = join(DOWNLOADS, tag);
// gh run download won't overwrite a file from an earlier download.
rmSync(folder, { recursive: true, force: true });
gh(['run', 'download', String(run.databaseId), '--repo', REPO, '--name', `lpdf-${tag}`, '--dir', folder]);
const vsix = join(folder, 'lpdf.vsix');

console.log('');
console.log(`Downloaded the ${tag} extension:`);
console.log(`  ${vsix}`);
console.log('');
console.log('1. Install it over the Marketplace version:');
console.log(`     code --install-extension "${vsix}" --force`);
console.log('   Without the code command: Extensions view, ... menu, Install from VSIX...');
console.log('2. Reload VS Code: Command Palette, Developer: Reload Window.');
console.log('   In the Extensions view, Lpdf now shows version 0.0.0: an RC build keeps');
console.log('   the placeholder version.');
console.log('3. Open an example, such as test/fixtures/showcase-toc.xml, then run');
console.log('   "Lpdf: Preview PDF" and "Lpdf: Export PDF", and open the exported PDF.');
