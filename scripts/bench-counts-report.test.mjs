// Tests of bench-counts-report.mjs: `node --test scripts/`. bench.yml runs them before it builds anything.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { compare, history, markdown, missingStages, readCounts } from './bench-counts-report.mjs';

/** A line of gungraun's JSON output, with only what the report reads. */
function summaryLine(stage, fixture, { ir, cycles = ir * 1.5, blocks = 100, bytes = 10_000 }) {
    const metrics = values => ({ metrics: Object.fromEntries(Object.entries(values).map(([k, v]) => [k, { values: { new: v } }])) });
    return JSON.stringify({
        function_name: stage,
        id: fixture,
        profiles: [
            { tool: 'Callgrind', data: { total: metrics({ Ir: ir, EstimatedCycles: cycles }) } },
            { tool: 'DHAT', data: { total: metrics({ TotalBlocks: blocks, TotalBytes: bytes }) } },
        ],
    });
}

const build = (name, lines) => ({ name, commit: `${name}0000000`, counts: readCounts(lines.join('\n')) });

test('reads the four measures of each fixture and stage', () => {
    const counts = readCounts([
        summaryLine('parse', 'xs', { ir: 1000, cycles: 1500, blocks: 7, bytes: 512 }),
        summaryLine('end_to_end', 'm', { ir: 9000 }),
    ].join('\n'));
    assert.deepEqual(counts.xs.parse, { instructions: 1000, estimated_cycles: 1500, allocations: 7, bytes_allocated: 512 });
    assert.equal(counts.m.end_to_end.instructions, 9000);
});

test('refuses a benchmark that DHAT did not measure', () => {
    const line = JSON.parse(summaryLine('parse', 'xs', { ir: 1000 }));
    line.profiles = line.profiles.filter(p => p.tool !== 'DHAT');
    assert.throws(() => readCounts(JSON.stringify(line)), /parse\.xs has no DHAT TotalBlocks/);
});

test('marks a whole render that grew past its 3% but not a stage that grew less than 10%', () => {
    const release = build('v0.29.0', [summaryLine('end_to_end', 'xs', { ir: 1000 }), summaryLine('parse', 'xs', { ir: 1000 })]);
    const branch = build('branch', [summaryLine('end_to_end', 'xs', { ir: 1040 }), summaryLine('parse', 'xs', { ir: 1090 })]);
    const rows = compare([release, branch]);
    const row = (stage, measure) => rows.find(r => r.stage === stage && r.measure === measure);
    assert.equal(row('end_to_end', 'instructions').changes[0].marked, true);
    assert.equal(row('end_to_end', 'estimated_cycles').changes[0].marked, true);
    assert.equal(row('parse', 'instructions').changes[0].marked, false);
    assert.equal(row('parse', 'instructions').changes[0].percent.toFixed(2), '9.00');
});

test('marks allocations that grew past 10% in any stage, and never a drop', () => {
    const release = build('v0.29.0', [summaryLine('parse', 's', { ir: 1000, blocks: 100, bytes: 1000 })]);
    const branch = build('branch', [summaryLine('parse', 's', { ir: 500, blocks: 111, bytes: 500 })]);
    const rows = compare([release, branch]);
    const marked = rows.filter(r => r.changes[0].marked).map(r => r.measure);
    assert.deepEqual(marked, ['allocations']);
});

test('takes the marks it is given', () => {
    const release = build('v0.29.0', [summaryLine('parse', 's', { ir: 1000 })]);
    const branch = build('branch', [summaryLine('parse', 's', { ir: 1020 })]);
    const rows = compare([release, branch], { endToEnd: 1, stage: 1, allocations: 1 });
    assert.equal(rows.find(r => r.measure === 'instructions').changes[0].marked, true);
});

test('compares three builds in two steps: the release to main, and main to the branch', () => {
    const rows = compare([
        build('v0.29.0', [summaryLine('parse', 'm', { ir: 1000 })]),
        build('main', [summaryLine('parse', 'm', { ir: 1200 })]),
        build('branch', [summaryLine('parse', 'm', { ir: 1200 })]),
    ]);
    const instructions = rows.find(r => r.measure === 'instructions');
    assert.deepEqual(instructions.values, [1000, 1200, 1200]);
    assert.deepEqual(instructions.changes.map(c => c.marked), [true, false]);
    assert.deepEqual(instructions.changes.map(c => Math.round(c.percent)), [20, 0]);
});

test('orders the rows by fixture size, then by pipeline stage', () => {
    const lines = ['xl', 'xs', 'm'].flatMap(f => ['end_to_end', 'layout', 'parse'].map(s => summaryLine(s, f, { ir: 1 })));
    const rows = compare([build('a', lines), build('b', lines)]).filter(r => r.measure === 'instructions');
    assert.deepEqual(rows.map(r => `${r.fixture} ${r.stage}`), [
        'xs parse', 'xs layout', 'xs end_to_end',
        'm parse', 'm layout', 'm end_to_end',
        'xl parse', 'xl layout', 'xl end_to_end',
    ]);
});

test('shows a stage the release lacks with no change, and says why', () => {
    const release = build('v0.29.0', [summaryLine('parse', 'xs', { ir: 1000 })]);
    const branch = build('branch', [summaryLine('parse', 'xs', { ir: 1000 }), summaryLine('layout', 'xs', { ir: 5000 })]);
    const rows = compare([release, branch]);
    const layout = rows.find(r => r.stage === 'layout' && r.measure === 'instructions');
    assert.deepEqual(layout.values, [undefined, 5000]);
    assert.equal(layout.changes[0].percent, undefined);
    assert.equal(layout.changes[0].marked, false);
    assert.deepEqual(missingStages([release, branch]), [{ build: 'v0.29.0', stages: ['layout'] }]);

    const report = markdown([release, branch], rows);
    assert.match(report, /`v0\.29\.0` has no layout benchmark/);
    assert.match(report, /\| xs \| layout \| instructions \| — \| 5,000 \| — \|/);
});

test('lists the marked rows above the whole table', () => {
    const release = build('v0.29.0', [summaryLine('end_to_end', 'l', { ir: 1_000_000 })]);
    const branch = build('branch', [summaryLine('end_to_end', 'l', { ir: 1_050_000 })]);
    const report = markdown([release, branch], compare([release, branch]), undefined, { rust: 'rustc 1.99.0' });
    assert.match(report, /all with rustc 1\.99\.0/);
    assert.match(report, /\*\*2 over the thresholds:\*\*/);
    assert.match(report, /\| l \| end to end \| instructions \| 1,000,000 \| 1,050,000 \| \*\*\+5\.00%\*\* ⚠ \|/);
});

test('says when nothing is over the thresholds', () => {
    const lines = [summaryLine('parse', 'xs', { ir: 1000 })];
    const report = markdown([build('v0.29.0', lines), build('branch', lines)], compare([build('v0.29.0', lines), build('branch', lines)]));
    assert.match(report, /\*\*Nothing over the thresholds\.\*\*/);
    assert.match(report, /\| xs \| parse \| instructions \| 1,000 \| 1,000 \| ±0\.00% \|/);
});

test('keeps the counts of the last build, against the build before it', () => {
    const before = build('v0.28.0', [summaryLine('parse', 'xs', { ir: 900 })]);
    const release = build('v0.29.0', [summaryLine('parse', 'xs', { ir: 1000 })]);
    const kept = history([before, release], { rust: 'rustc 1.99.0' });
    assert.equal(kept.version, 'v0.29.0');
    assert.equal(kept.commit, 'v0.29.00000000');
    assert.equal(kept.rust, 'rustc 1.99.0');
    assert.equal(kept.counts.xs.parse.instructions, 1000);
    assert.equal(kept.against.version, 'v0.28.0');
    assert.equal(kept.against.counts.xs.parse.instructions, 900);
});
