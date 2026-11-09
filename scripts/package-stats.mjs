#!/usr/bin/env node
// Shows how often Lpdf's packages were downloaded, from each registry's public stats API,
// in one table. Nothing is saved: every run asks the registries again.
//
//   make package-stats                              the last 30 days
//   make package-stats DAYS=90                      the last 90 days
//   make package-stats FROM=2026-10-01 TO=2026-10-31   a date range, both days included
//
// The table has the downloads in the range and the all-time total. What each registry can tell:
//   npm, Packagist  downloads per day, so any range, and the all-time total
//   PyPI            downloads per day, counted without mirrors; its data is only the last ~180
//                   days, so its all-time total is the sum of the data it still has
//   NuGet           one all-time total, no range, so no number for the range
//   Maven Central   no public stats API: read it on the Central Portal dashboard
// A registry that cannot be reached shows its error, and the exit code is 1.
// No token and no login: the four APIs are public.

import { pathToFileURL } from 'node:url';

const DEFAULT_DAYS = 30;
const DAY_RE = /^\d{4}-\d{2}-\d{2}$/;
const DAY_MS = 24 * 60 * 60 * 1000;

const PACKAGES = {
    npm: '@lpdfio/lpdf',
    pypi: 'lpdfio-lpdf',
    packagist: 'lpdfio/lpdf',
    nuget: 'Lpdfio.Lpdf',
    maven: 'io.lpdf:lpdf',
};

const isDay = text => DAY_RE.test(text) && !Number.isNaN(Date.parse(text));
const dayOf = ms => new Date(ms).toISOString().slice(0, 10);

// Turns the command line into { from, to }, both 'YYYY-MM-DD'. Throws a message on a bad one.
export function parseRange(args, now = Date.now()) {
    const options = {};
    for (let i = 0; i < args.length; i += 2) {
        const name = args[i];
        if (!['--from', '--to', '--days'].includes(name)) throw new Error(`unknown option '${name}'`);
        if (args[i + 1] === undefined) throw new Error(`${name} needs a value`);
        options[name.slice(2)] = args[i + 1];
    }
    if (options.days !== undefined && (options.from || options.to)) {
        throw new Error('use --days, or --from and --to, not both');
    }
    const today = dayOf(now);
    if (options.from === undefined && options.to === undefined) {
        const days = options.days === undefined ? DEFAULT_DAYS : Number(options.days);
        if (!Number.isInteger(days) || days < 1) throw new Error(`--days must be a whole number of 1 or more, got '${options.days}'`);
        return { from: dayOf(now - (days - 1) * DAY_MS), to: today };
    }
    const from = options.from ?? dayOf(Date.parse(options.to) - (DEFAULT_DAYS - 1) * DAY_MS);
    const to = options.to ?? today;
    for (const [name, value] of [['--from', from], ['--to', to]]) {
        if (!isDay(value)) throw new Error(`${name} must look like 2026-10-31, got '${value}'`);
    }
    if (from > to) throw new Error(`--from ${from} is after --to ${to}`);
    return { from, to };
}

// Adds up { day: count } entries whose day is inside the range, both ends included.
export function sumInRange(entries, { from, to }) {
    let total = 0;
    for (const [day, count] of entries) {
        if (day >= from && day <= to) total += count;
    }
    return total;
}

async function fetchJson(url) {
    const res = await fetch(url, {
        headers: { Accept: 'application/json', 'User-Agent': 'lpdf-package-stats' },
        signal: AbortSignal.timeout(20_000),
    });
    if (!res.ok) throw new Error(`${url} answered ${res.status}`);
    return res.json();
}

// ── One function per registry: (range, getJson) → { downloads, note } ───────────

// npm's range endpoints answer for at most 18 months, so an older package counts its last 540 days.
const NPM_MAX_DAYS = 540;

async function npmStats(range, getJson, now) {
    const data = await getJson(`https://api.npmjs.org/downloads/range/${range.from}:${range.to}/${PACKAGES.npm}`);
    const created = (await getJson(`https://registry.npmjs.org/${PACKAGES.npm.replace('/', '%2f')}`)).time.created.slice(0, 10);
    const oldest = dayOf(now - NPM_MAX_DAYS * DAY_MS);
    const start = created < oldest ? oldest : created;
    const all = await getJson(`https://api.npmjs.org/downloads/point/${start}:${dayOf(now)}/${PACKAGES.npm}`);
    return {
        downloads: sumInRange(data.downloads.map(d => [d.day, d.downloads]), range),
        allTime: all.downloads,
        note: start === created ? '' : `all time counts from ${start}, npm answers for 18 months`,
    };
}

async function pypiStats(range, getJson) {
    const data = (await getJson(`https://pypistats.org/api/packages/${PACKAGES.pypi}/overall`)).data;
    const count = category => sumInRange(data.filter(d => d.category === category).map(d => [d.date, d.downloads]), range);
    const first = data.map(d => d.date).sort()[0];
    const everything = { from: '0000-01-01', to: '9999-12-31' };
    const allTime = sumInRange(data.filter(d => d.category === 'without_mirrors').map(d => [d.date, d.downloads]), everything);
    const notes = [`with mirrors ${count('with_mirrors')}`, `data starts ${first}`];
    return { downloads: count('without_mirrors'), allTime, note: notes.join('; ') };
}

async function packagistStats(range, getJson) {
    const data = await getJson(`https://packagist.org/packages/${PACKAGES.packagist}/stats/all.json?average=daily&from=${range.from}&to=${range.to}`);
    const values = data.values[PACKAGES.packagist] ?? [];
    const total = (await getJson(`https://packagist.org/packages/${PACKAGES.packagist}.json`)).package.downloads.total;
    return { downloads: sumInRange(data.labels.map((day, i) => [day, values[i] ?? 0]), range), allTime: total, note: '' };
}

async function nugetStats(range, getJson) {
    const data = await getJson(`https://azuresearch-usnc.nuget.org/query?q=packageid:${PACKAGES.nuget.toLowerCase()}&prerelease=true&take=1`);
    const found = data.data.find(d => d.id.toLowerCase() === PACKAGES.nuget.toLowerCase());
    if (!found) throw new Error(`${PACKAGES.nuget} not found on NuGet`);
    return { downloads: undefined, allTime: found.totalDownloads, note: 'NuGet has no per-day counts' };
}

async function mavenStats() {
    return { downloads: undefined, allTime: undefined, note: 'no public stats API: see the Central Portal dashboard' };
}

const REGISTRIES = [
    ['npm', npmStats],
    ['PyPI', pypiStats],
    ['Packagist', packagistStats],
    ['NuGet', nugetStats],
    ['Maven Central', mavenStats],
];
const PACKAGE_OF = { npm: 'npm', PyPI: 'pypi', Packagist: 'packagist', NuGet: 'nuget', 'Maven Central': 'maven' };

// Asks every registry at once. A failed one becomes { error } instead of stopping the rest.
export async function collect(range, getJson = fetchJson, now = Date.now()) {
    const results = await Promise.allSettled(REGISTRIES.map(([, stats]) => stats(range, getJson, now)));
    return REGISTRIES.map(([registry], i) => {
        const base = { registry, package: PACKAGES[PACKAGE_OF[registry]] };
        const result = results[i];
        return result.status === 'fulfilled' ? { ...base, ...result.value } : { ...base, error: result.reason.message };
    });
}

// The table as text. Counts are right-aligned; a registry without a count shows '-'.
export function renderTable(rows, range) {
    const count = value => (value === undefined ? '-' : String(value));
    const head = ['Registry', 'Package', 'Downloads', 'All time', 'Note'];
    const body = rows.map(r => [
        r.registry,
        r.package,
        r.error ? 'error' : count(r.downloads),
        r.error ? 'error' : count(r.allTime),
        r.error ?? r.note,
    ]);
    const sum = key => rows.reduce((total, r) => total + (r.error ? 0 : (r[key] ?? 0)), 0);
    const failed = rows.filter(r => r.error).length;
    const total = ['Total', '', String(sum('downloads')), String(sum('allTime')), failed ? `${failed} of ${rows.length} registries failed` : ''];
    const widths = head.map((_, c) => Math.max(...[head, ...body, total].map(row => row[c].length)));
    const format = row => row.map((cell, c) => (c === 2 || c === 3 ? cell.padStart(widths[c]) : cell.padEnd(widths[c]))).join('  ').trimEnd();
    const rule = widths.map(w => '-'.repeat(w)).join('  ');
    return [`Lpdf downloads, ${range.from} to ${range.to}`, '', format(head), rule, ...body.map(format), rule, format(total)].join('\n');
}

async function main() {
    let range;
    try {
        range = parseRange(process.argv.slice(2));
    } catch (error) {
        console.error(`package-stats: ${error.message}`);
        process.exitCode = 1;
        return;
    }
    const rows = await collect(range);
    console.log(renderTable(rows, range));
    // exitCode, not process.exit(): let Node close the fetch sockets itself.
    if (rows.some(r => r.error)) process.exitCode = 1;
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) await main();
