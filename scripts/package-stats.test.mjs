// Tests of package-stats.mjs: node --test scripts/package-stats.test.mjs

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { collect, parseRange, renderTable, sumInRange } from './package-stats.mjs';

const NOW = Date.parse('2026-11-09T12:00:00Z');

test('parseRange defaults to the last 30 days, today included', () => {
    assert.deepEqual(parseRange([], NOW), { from: '2026-10-11', to: '2026-11-09' });
});

test('parseRange takes --days, or --from and --to', () => {
    assert.deepEqual(parseRange(['--days', '1'], NOW), { from: '2026-11-09', to: '2026-11-09' });
    assert.deepEqual(parseRange(['--from', '2026-10-01', '--to', '2026-10-31'], NOW), { from: '2026-10-01', to: '2026-10-31' });
    assert.deepEqual(parseRange(['--from', '2026-10-01'], NOW), { from: '2026-10-01', to: '2026-11-09' });
});

test('parseRange refuses what it cannot read', () => {
    assert.throws(() => parseRange(['--days', '0'], NOW), /whole number/);
    assert.throws(() => parseRange(['--from', '2026-13-01'], NOW), /must look like/);
    assert.throws(() => parseRange(['--from', '2026-11-02', '--to', '2026-11-01'], NOW), /after/);
    assert.throws(() => parseRange(['--days', '5', '--from', '2026-11-01'], NOW), /not both/);
    assert.throws(() => parseRange(['--week', '1'], NOW), /unknown option/);
    assert.throws(() => parseRange(['--days'], NOW), /needs a value/);
});

test('sumInRange counts both ends and skips days outside', () => {
    const entries = [['2026-09-30', 100], ['2026-10-01', 1], ['2026-10-02', 2], ['2026-10-03', 4]];
    assert.equal(sumInRange(entries, { from: '2026-10-01', to: '2026-10-02' }), 3);
});

const range = { from: '2026-10-01', to: '2026-10-03' };
const fakeApi = url => {
    if (url.startsWith('https://api.npmjs.org/downloads/point/')) return { downloads: 500 };
    if (url.startsWith('https://registry.npmjs.org/')) return { time: { created: '2026-04-26T05:59:41.714Z' } };
    if (url.startsWith('https://api.npmjs.org/')) {
        return { downloads: [{ day: '2026-10-01', downloads: 5 }, { day: '2026-10-02', downloads: 6 }] };
    }
    if (url.startsWith('https://pypistats.org/')) {
        return {
            data: [
                { category: 'with_mirrors', date: '2026-10-02', downloads: 9 },
                { category: 'without_mirrors', date: '2026-10-02', downloads: 7 },
                { category: 'without_mirrors', date: '2026-09-01', downloads: 100 },
            ],
        };
    }
    if (url === 'https://packagist.org/packages/lpdfio/lpdf.json') return { package: { downloads: { total: 94 } } };
    if (url.startsWith('https://packagist.org/')) {
        return { labels: ['2026-10-01', '2026-10-02'], values: { 'lpdfio/lpdf': [1, 2] } };
    }
    if (url.startsWith('https://azuresearch')) return { data: [{ id: 'Lpdfio.Lpdf', totalDownloads: 4177 }] };
    throw new Error(`unexpected ${url}`);
};

test('collect gives one row per registry, with counts by day where the registry has them', async () => {
    const rows = await collect(range, async url => fakeApi(url));
    assert.deepEqual(rows.map(r => [r.registry, r.downloads, r.allTime]), [
        ['npm', 11, 500], ['PyPI', 7, 107], ['Packagist', 3, 94], ['NuGet', undefined, 4177], ['Maven Central', undefined, undefined],
    ], 'PyPI all time is the sum of its data');
    assert.match(rows[1].note, /with mirrors 9/);
});

test('a registry that fails shows its error and the others still report', async () => {
    const rows = await collect(range, async url => {
        if (url.includes('npmjs.org')) throw new Error('npm is down');
        return fakeApi(url);
    });
    assert.equal(rows[0].error, 'npm is down');
    assert.equal(rows[2].downloads, 3);
    const text = renderTable(rows, range);
    assert.match(text, /npm +@lpdfio\/lpdf +error +error +npm is down/);
    assert.match(text, /Total +10 +4378 +1 of 5 registries failed/);
});

test('an old npm package counts all time from the oldest day npm answers for', async () => {
    const urls = [];
    const getJson = async url => {
        urls.push(url);
        if (url.startsWith('https://registry.npmjs.org/')) return { time: { created: '2020-01-01T00:00:00Z' } };
        return url.includes('/point/') ? { downloads: 1 } : { downloads: [] };
    };
    const rows = await collect(range, getJson, NOW);
    assert.match(urls.find(u => u.includes('/point/')), /point\/2025-05-18:2026-11-09\//);
    assert.match(rows.find(r => r.registry === 'npm').note, /counts from 2025-05-18/);
});
