// Records which core the lpdf.io docs describe, for the docs header to show: the last release this
// checkout is built on, how many commits it is past that release, and the commit. Run by
// `make build-pages` and `make dev-pages`, which are what copy the engine into the pages tree.
//
// It warns when the engine that was copied does not match the checkout, because the stamp would
// then name a core the docs were not checked against.
//
// Usage: node scripts/stamp-docs.mjs <output.json>

import { execFileSync } from 'node:child_process';
import { statSync, writeFileSync } from 'node:fs';

const out = process.argv[2];
if (!out)
{
    console.error('usage: node scripts/stamp-docs.mjs <output.json>');
    process.exit(1);
}

const git = (...args) => execFileSync('git', args, { encoding: 'utf8' }).trim();

// Release candidates are not releases: the docs describe a release.
const release = git('describe', '--tags', '--match', 'v[0-9]*', '--exclude', '*-rc*', '--abbrev=0');
const version = /^v(\d+)\.(\d+)\.\d+$/.exec(release);
if (!version)
{
    console.error(`cannot read a release version from the tag '${release}'`);
    process.exit(1);
}

const stamp = {
    engine: `${version[1]}.${version[2]}`,
    release,
    ahead: Number(git('rev-list', '--count', `${release}..HEAD`)),
    commit: git('rev-parse', '--short', 'HEAD'),
};
writeFileSync(out, `${JSON.stringify(stamp, null, 4)}\n`);

const past = stamp.ahead ? `, ${stamp.ahead} commit${stamp.ahead === 1 ? '' : 's'} past ${release}` : '';
console.log(`>>> ${out}: engine ${stamp.engine} (${stamp.commit}${past})`);

// The copied engine is dist/web, so it should be newer than the last change to the core.
try
{
    const built = statSync('dist/web/lpdf_bg.wasm').mtimeMs;
    const changed = Number(git('log', '-1', '--format=%ct', '--', 'src/core')) * 1000;
    if (built < changed)
        console.warn('WARNING: dist/web is older than the last change to src/core. Run `make build-wasm` so the docs are checked against the engine the stamp names.');
    if (git('status', '--porcelain', '--', 'src/core'))
        console.warn('WARNING: src/core has uncommitted changes, so the stamped commit is not exactly the engine that was built.');
}
catch
{
    console.warn('WARNING: could not compare dist/web with the checkout.');
}
