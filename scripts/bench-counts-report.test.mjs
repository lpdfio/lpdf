// Tests of bench-counts-report.mjs: `node --test scripts/`. bench.yml runs them before it builds anything.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { compare, history, markdown, missingFixtures, missingStages, readCounts } from './bench-counts-report.mjs';

/**
 * The two lines of one benchmark, with only what the report reads: gungraun's JSON line for the work, and the
 * counting allocator's for the memory.
 */
function benchmark(stage, fixture, { ir, cycles = ir * 1.5, blocks = 100, bytes = 10_000, peak = 4_000 }) {
    const metrics = values => ({ metrics: Object.fromEntries(Object.entries(values).map(([k, v]) => [k, { values: { new: v } }])) });
    return {
        summary: JSON.stringify({
            function_name: stage,
            id: fixture,
            profiles: [{ tool: 'Callgrind', data: { total: metrics({ Ir: ir, EstimatedCycles: cycles }) } }],
        }),
        allocations: JSON.stringify({ stage, fixture, allocations: blocks, bytes_allocated: bytes, peak_bytes: peak }),
    };
}

/** The counts of a build's benchmarks, read from the two files they make. */
const readBenchmarks = benchmarks => readCounts(
    benchmarks.map(b => b.summary).join('\n'),
    benchmarks.map(b => b.allocations).join('\n'),
);

const build = (name, benchmarks) => ({ name, commit: `${name}0000000`, counts: readBenchmarks(benchmarks) });

test('reads the five measures of each fixture and stage from the two files', () => {
    const counts = readBenchmarks([
        benchmark('parse', 'xs', { ir: 1000, cycles: 1500, blocks: 7, bytes: 512, peak: 256 }),
        benchmark('end_to_end', 'm', { ir: 9000 }),
    ]);
    assert.deepEqual(counts.xs.parse, { instructions: 1000, estimated_cycles: 1500, allocations: 7, bytes_allocated: 512, peak_bytes: 256 });
    assert.equal(counts.m.end_to_end.instructions, 9000);
});

test('refuses a benchmark that Callgrind did not measure', () => {
    const { summary, allocations } = benchmark('parse', 'xs', { ir: 1000 });
    const line = JSON.parse(summary);
    line.profiles = [];
    assert.throws(() => readCounts(JSON.stringify(line), allocations), /parse\.xs has no Callgrind Ir/);
});

test('refuses a benchmark that is in one file and not the other', () => {
    const parse = benchmark('parse', 'xs', { ir: 1000 });
    const layout = benchmark('layout', 'xs', { ir: 1000 });
    assert.throws(() => readCounts(parse.summary, `${parse.allocations}\n${layout.allocations}`), /layout\.xs has allocations but no instructions/);
    assert.throws(() => readCounts(`${parse.summary}\n${layout.summary}`, parse.allocations), /layout\.xs has instructions but no allocations/);
});

test('refuses an allocation line without its peak', () => {
    const { summary, allocations } = benchmark('parse', 'xs', { ir: 1000 });
    const line = JSON.parse(allocations);
    delete line.peak_bytes;
    assert.throws(() => readCounts(summary, JSON.stringify(line)), /parse\.xs has no peak_bytes/);
});

test('marks a whole render that grew past its 3% but not a stage that grew less than 10%', () => {
    const release = build('v0.29.0', [benchmark('end_to_end', 'xs', { ir: 1000 }), benchmark('parse', 'xs', { ir: 1000 })]);
    const branch = build('branch', [benchmark('end_to_end', 'xs', { ir: 1040 }), benchmark('parse', 'xs', { ir: 1090 })]);
    const rows = compare([release, branch]);
    const row = (stage, measure) => rows.find(r => r.stage === stage && r.measure === measure);
    assert.equal(row('end_to_end', 'instructions').changes[0].marked, true);
    assert.equal(row('end_to_end', 'estimated_cycles').changes[0].marked, true);
    assert.equal(row('parse', 'instructions').changes[0].marked, false);
    assert.equal(row('parse', 'instructions').changes[0].percent.toFixed(2), '9.00');
});

test('marks allocations or peak memory that grew past 10% in any stage, and never a drop', () => {
    const release = build('v0.29.0', [benchmark('parse', 's', { ir: 1000, blocks: 100, bytes: 1000, peak: 400 })]);
    const branch = build('branch', [benchmark('parse', 's', { ir: 500, blocks: 111, bytes: 500, peak: 450 })]);
    const rows = compare([release, branch]);
    const marked = rows.filter(r => r.changes[0].marked).map(r => r.measure);
    assert.deepEqual(marked, ['allocations', 'peak_bytes']);
});

test('takes the marks it is given', () => {
    const release = build('v0.29.0', [benchmark('parse', 's', { ir: 1000 })]);
    const branch = build('branch', [benchmark('parse', 's', { ir: 1020 })]);
    const rows = compare([release, branch], { endToEnd: 1, stage: 1, allocations: 1 });
    assert.equal(rows.find(r => r.measure === 'instructions').changes[0].marked, true);
});

test('compares three builds in two steps: the release to main, and main to the branch', () => {
    const rows = compare([
        build('v0.29.0', [benchmark('parse', 'm', { ir: 1000 })]),
        build('main', [benchmark('parse', 'm', { ir: 1200 })]),
        build('branch', [benchmark('parse', 'm', { ir: 1200 })]),
    ]);
    const instructions = rows.find(r => r.measure === 'instructions');
    assert.deepEqual(instructions.values, [1000, 1200, 1200]);
    assert.deepEqual(instructions.changes.map(c => c.marked), [true, false]);
    assert.deepEqual(instructions.changes.map(c => Math.round(c.percent)), [20, 0]);
});

test('orders the rows by fixture size, then by pipeline stage', () => {
    const lines = ['xl', 'xs', 'm'].flatMap(f => ['end_to_end', 'layout', 'parse'].map(s => benchmark(s, f, { ir: 1 })));
    const rows = compare([build('a', lines), build('b', lines)]).filter(r => r.measure === 'instructions');
    assert.deepEqual(rows.map(r => `${r.fixture} ${r.stage}`), [
        'xs parse', 'xs layout', 'xs end_to_end',
        'm parse', 'm layout', 'm end_to_end',
        'xl parse', 'xl layout', 'xl end_to_end',
    ]);
});

test('shows a stage the release lacks with no change, and says why', () => {
    const release = build('v0.29.0', [benchmark('parse', 'xs', { ir: 1000 })]);
    const branch = build('branch', [benchmark('parse', 'xs', { ir: 1000 }), benchmark('layout', 'xs', { ir: 5000 })]);
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

test('puts the fixtures of one feature after the sizes, and data binding after parse', () => {
    const lines = [
        benchmark('parse', 'code', { ir: 1 }), benchmark('parse', 'data', { ir: 1 }), benchmark('data', 'data', { ir: 1 }),
        benchmark('parse', 'xl', { ir: 1 }), benchmark('parse', 'justified', { ir: 1 }), benchmark('layout', 'data', { ir: 1 }),
    ];
    const rows = compare([build('a', lines), build('b', lines)]).filter(r => r.measure === 'instructions');
    assert.deepEqual(rows.map(r => `${r.fixture} ${r.stage}`), [
        'xl parse', 'justified parse', 'data parse', 'data data', 'data layout', 'code parse',
    ]);
});

test('shows a fixture the release lacks with no change, and says it has no baseline', () => {
    const release = build('v0.29.0', [benchmark('parse', 'm', { ir: 1000 })]);
    const branch = build('branch', [
        benchmark('parse', 'm', { ir: 1000 }), benchmark('parse', 'markdown', { ir: 7000 }),
        benchmark('parse', 'code', { ir: 3000 }), benchmark('parse', 'fonts', { ir: 5000 }),
    ]);
    const rows = compare([release, branch]);
    const markdownRow = rows.find(r => r.fixture === 'markdown' && r.measure === 'instructions');
    assert.deepEqual(markdownRow.values, [undefined, 7000]);
    assert.equal(markdownRow.changes[0].marked, false);
    assert.deepEqual(missingFixtures([release, branch]), [{ build: 'v0.29.0', fixtures: ['markdown', 'code', 'fonts'] }]);
    assert.deepEqual(missingStages([release, branch]), []);

    const report = markdown([release, branch], rows);
    assert.match(report, /`v0\.29\.0` has no `markdown`, `code` or `fonts` fixture, so those rows have no baseline: they compare from the first release that has them\./);
    assert.match(report, /\| markdown \| parse \| instructions \| — \| 7,000 \| — \|/);
});

test('lists the marked rows above the whole table', () => {
    const release = build('v0.29.0', [benchmark('end_to_end', 'l', { ir: 1_000_000 })]);
    const branch = build('branch', [benchmark('end_to_end', 'l', { ir: 1_050_000 })]);
    const report = markdown([release, branch], compare([release, branch]), undefined, { rust: 'rustc 1.99.0' });
    assert.match(report, /all with rustc 1\.99\.0/);
    assert.match(report, /peak memory from a counting allocator/);
    assert.match(report, /\*\*2 over the thresholds:\*\*/);
    assert.match(report, /\| l \| end to end \| instructions \| 1,000,000 \| 1,050,000 \| \*\*\+5\.00%\*\* ⚠ \|/);
});

test('says when nothing is over the thresholds', () => {
    const lines = [benchmark('parse', 'xs', { ir: 1000 })];
    const report = markdown([build('v0.29.0', lines), build('branch', lines)], compare([build('v0.29.0', lines), build('branch', lines)]));
    assert.match(report, /\*\*Nothing over the thresholds\.\*\*/);
    assert.match(report, /\| xs \| parse \| instructions \| 1,000 \| 1,000 \| ±0\.00% \|/);
});

test('keeps the counts of the last build, against the build before it', () => {
    const before = build('v0.28.0', [benchmark('parse', 'xs', { ir: 900 })]);
    const release = build('v0.29.0', [benchmark('parse', 'xs', { ir: 1000 })]);
    const kept = history([before, release], { rust: 'rustc 1.99.0' });
    assert.equal(kept.version, 'v0.29.0');
    assert.equal(kept.commit, 'v0.29.00000000');
    assert.equal(kept.rust, 'rustc 1.99.0');
    assert.equal(kept.counts.xs.parse.instructions, 1000);
    assert.equal(kept.against.version, 'v0.28.0');
    assert.equal(kept.against.counts.xs.parse.instructions, 900);
});
