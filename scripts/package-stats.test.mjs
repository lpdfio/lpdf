// Tests of package-stats.mjs: node --test scripts/package-stats.test.mjs

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { collect, renderTable } from './package-stats.mjs';

const NOW = Date.parse('2026-11-09T12:00:00Z');

const fakeApi = url => {
    if (url.startsWith('https://registry.npmjs.org/')) return { time: { created: '2026-04-26T05:59:41.714Z' } };
    if (url.startsWith('https://api.npmjs.org/')) return { downloads: 500 };
    if (url.startsWith('https://pypistats.org/')) {
        return {
            data: [
                { category: 'with_mirrors', date: '2026-10-02', downloads: 9000 },
                { category: 'without_mirrors', date: '2026-10-02', downloads: 7 },
                { category: 'without_mirrors', date: '2026-09-01', downloads: 100 },
            ],
        };
    }
    if (url === 'https://packagist.org/packages/lpdfio/lpdf.json') return { package: { downloads: { total: 94 } } };
    if (url.startsWith('https://azuresearch')) return { data: [{ id: 'Lpdfio.Lpdf', totalDownloads: 4177 }] };
    if (url.startsWith('https://marketplace.visualstudio.com/')) {
        const stats = [['install', 12], ['updateCount', 38], ['downloadCount', 334]].map(([statisticName, value]) => ({ statisticName, value }));
        return { results: [{ extensions: [{ statistics: stats }] }] };
    }
    if (url === 'https://open-vsx.org/api/lpdfio/lpdf') return { downloadCount: 944 };
    throw new Error(`unexpected ${url}`);
};

test('collect gives one row per registry with its all-time total', async () => {
    const rows = await collect(async url => fakeApi(url), NOW);
    assert.deepEqual(rows.map(r => [r.registry, r.downloads]), [
        ['npm', 500], ['PyPI', 107], ['Packagist', 94], ['NuGet', 4177],
        ['Maven Central', undefined], ['VS Marketplace', 334], ['Open VSX', 944],
    ]);
    assert.match(rows[1].note, /with mirrors 9,000; data starts 2026-09-01/);
    assert.deepEqual(rows[1].daily, { '2026-10-02': 7, '2026-09-01': 100 }, 'PyPI daily counts, without mirrors');
    assert.match(rows[5].note, /installs 12, updates 38/);
});

test('a registry that fails shows its error and the others still report', async () => {
    const rows = await collect(async url => {
        if (url.includes('npmjs.org')) throw new Error('npm is down');
        return fakeApi(url);
    }, NOW);
    assert.equal(rows[0].error, 'npm is down');
    assert.equal(rows[2].downloads, 94);
    const text = renderTable(rows);
    assert.match(text, /npm +@lpdfio\/lpdf +error +npm is down/);
    assert.match(text, /Total +5,656 +1 of 7 registries failed/);
});

test('the table shows a dash for a registry with no count, and a total that skips it', async () => {
    const text = renderTable(await collect(async url => fakeApi(url), NOW));
    assert.match(text, /Maven Central +io\.lpdf:lpdf +- +no public stats API/);
    assert.match(text, /Total +6,156\s*$/);
    assert.match(text, /Open VSX +lpdfio\.lpdf +944/);
    assert.match(text, /NuGet +Lpdfio\.Lpdf +4,177/);
});

test('an old npm package counts from the oldest day npm answers for', async () => {
    const urls = [];
    const getJson = async url => {
        urls.push(url);
        return url.startsWith('https://registry.npmjs.org/') ? { time: { created: '2020-01-01T00:00:00Z' } } : { downloads: 1 };
    };
    const rows = await collect(getJson, NOW);
    assert.match(urls.find(u => u.includes('/downloads/point/')), /point\/2025-05-18:2026-11-09\//);
    assert.match(rows[0].note, /counts from 2025-05-18/);
});
