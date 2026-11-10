#!/usr/bin/env node
// Shows how often Lpdf's packages were downloaded, all time, from each registry's public stats
// API, in one table. Nothing is saved: every run asks the registries again.
//
//   make package-stats
//
// collect() also gives PyPI's counts by day (`daily`, without mirrors), so a caller can keep its own history.
// What each registry can tell:
//   npm, Packagist, NuGet, Open VSX   the all-time total
//   VS Marketplace  the all-time installs; its downloads and updates are in the note
//   PyPI            the sum of the data pypistats still has, about the last 180 days, counted
//                   without mirrors; the count with mirrors is in the note
//   Maven Central   no public stats API: read it on the Central Portal dashboard
// A registry that cannot be reached shows its error, and the exit code is 1.
// No token and no login: the APIs are public.

import { pathToFileURL } from 'node:url';

const DAY_MS = 24 * 60 * 60 * 1000;

const PACKAGES = {
    npm: '@lpdfio/lpdf',
    pypi: 'lpdfio-lpdf',
    packagist: 'lpdfio/lpdf',
    nuget: 'Lpdfio.Lpdf',
    maven: 'io.lpdf:lpdf',
    marketplace: 'lpdfio.lpdf',
    openvsx: 'lpdfio.lpdf',
};

const grouped = count => count.toLocaleString('en-US');
const dayOf = ms => new Date(ms).toISOString().slice(0, 10);

async function fetchJson(url, init = {}) {
    const res = await fetch(url, {
        ...init,
        headers: { Accept: 'application/json', 'User-Agent': 'lpdf-package-stats', ...init.headers },
        signal: AbortSignal.timeout(20_000),
    });
    if (!res.ok) throw new Error(`${url} answered ${res.status}`);
    return res.json();
}

// ── One function per registry: (getJson, now) → { downloads, note } ─────────────

// npm's download endpoints answer for at most 18 months, so an older package counts its last 540 days.
const NPM_MAX_DAYS = 540;

async function npmStats(getJson, now) {
    const created = (await getJson(`https://registry.npmjs.org/${PACKAGES.npm.replace('/', '%2f')}`)).time.created.slice(0, 10);
    const oldest = dayOf(now - NPM_MAX_DAYS * DAY_MS);
    const start = created < oldest ? oldest : created;
    const all = await getJson(`https://api.npmjs.org/downloads/point/${start}:${dayOf(now)}/${PACKAGES.npm}`);
    return { downloads: all.downloads, note: start === created ? '' : `counts from ${start}, npm answers for 18 months` };
}

async function pypiStats(getJson) {
    const data = (await getJson(`https://pypistats.org/api/packages/${PACKAGES.pypi}/overall`)).data;
    const total = category => data.filter(d => d.category === category).reduce((sum, d) => sum + d.downloads, 0);
    const first = data.map(d => d.date).sort()[0];
    const daily = Object.fromEntries(data.filter(d => d.category === 'without_mirrors').map(d => [d.date, d.downloads]));
    return { downloads: total('without_mirrors'), daily, note: `with mirrors ${grouped(total('with_mirrors'))}; data starts ${first}` };
}

async function packagistStats(getJson) {
    const data = await getJson(`https://packagist.org/packages/${PACKAGES.packagist}.json`);
    return { downloads: data.package.downloads.total, note: '' };
}

async function nugetStats(getJson) {
    const id = PACKAGES.nuget.toLowerCase();
    const data = await getJson(`https://azuresearch-usnc.nuget.org/query?q=packageid:${id}&prerelease=true&take=1`);
    const found = data.data.find(d => d.id.toLowerCase() === id);
    if (!found) throw new Error(`${PACKAGES.nuget} not found on NuGet`);
    return { downloads: found.totalDownloads, note: '' };
}

async function mavenStats() {
    return { downloads: undefined, note: 'no public stats API: see the Central Portal dashboard' };
}

// The number is the Marketplace's installs, the figure its page shows. Its downloads and updates go in the note.
async function marketplaceStats(getJson) {
    const id = PACKAGES.marketplace;
    const data = await getJson('https://marketplace.visualstudio.com/_apis/public/gallery/extensionquery', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Accept: 'application/json;api-version=3.0-preview.1' },
        body: JSON.stringify({ filters: [{ criteria: [{ filterType: 7, value: id }] }], flags: 914 }),
    });
    const found = data.results[0].extensions[0];
    if (!found) throw new Error(`${id} not found on the VS Code Marketplace`);
    const stat = name => found.statistics.find(s => s.statisticName === name)?.value ?? 0;
    return { downloads: stat('install'), note: `downloads ${grouped(stat('downloadCount'))}, updates ${grouped(stat('updateCount'))}` };
}

async function openVsxStats(getJson) {
    const data = await getJson(`https://open-vsx.org/api/${PACKAGES.openvsx.replace('.', '/')}`);
    if (typeof data.downloadCount !== 'number') throw new Error(`${PACKAGES.openvsx} not found on Open VSX`);
    return { downloads: data.downloadCount, note: '' };
}

const REGISTRIES = [
    ['npm', 'npm', npmStats],
    ['PyPI', 'pypi', pypiStats],
    ['Packagist', 'packagist', packagistStats],
    ['NuGet', 'nuget', nugetStats],
    ['Maven Central', 'maven', mavenStats],
    ['VS Marketplace', 'marketplace', marketplaceStats],
    ['Open VSX', 'openvsx', openVsxStats],
];

// Asks every registry at once. A failed one becomes { error } instead of stopping the rest.
export async function collect(getJson = fetchJson, now = Date.now()) {
    const results = await Promise.allSettled(REGISTRIES.map(([, , stats]) => stats(getJson, now)));
    return REGISTRIES.map(([registry, key], i) => {
        const base = { registry, package: PACKAGES[key] };
        const result = results[i];
        return result.status === 'fulfilled' ? { ...base, ...result.value } : { ...base, error: result.reason.message };
    });
}

// The table as text. Counts are right-aligned with thousands separators; a registry without a count shows '-'.
export function renderTable(rows) {
    const head = ['Registry', 'Package', 'Downloads', 'Note'];
    const body = rows.map(r => [
        r.registry,
        r.package,
        r.error ? 'error' : r.downloads === undefined ? '-' : grouped(r.downloads),
        r.error ?? r.note,
    ]);
    const sum = rows.reduce((total, r) => total + (r.error ? 0 : (r.downloads ?? 0)), 0);
    const failed = rows.filter(r => r.error).length;
    const total = ['Total', '', grouped(sum), failed ? `${failed} of ${rows.length} registries failed` : ''];
    const widths = head.map((_, c) => Math.max(...[head, ...body, total].map(row => row[c].length)));
    const format = row => row.map((cell, c) => (c === 2 ? cell.padStart(widths[c]) : cell.padEnd(widths[c]))).join('  ').trimEnd();
    const rule = widths.map(w => '-'.repeat(w)).join('  ');
    return ['Lpdf downloads, all time', '', format(head), rule, ...body.map(format), rule, format(total)].join('\n');
}

async function main() {
    const rows = await collect();
    console.log(renderTable(rows));
    // exitCode, not process.exit(): let Node close the fetch sockets itself.
    if (rows.some(r => r.error)) process.exitCode = 1;
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) await main();
