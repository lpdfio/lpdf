// Tests of bench-counts-report.mjs: `node --test scripts/`. bench.yml runs them before it measures anything, with
// LPDF_CLI set to the command line it built, so the template renders the JSON of a test of two builds and of three.
// Without LPDF_CLI those two tests are skipped.

import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

import {
    CHANGE_COLORS, STAGES, compare, history, json, markdown, measureTables, missingFixtures, missingStages, readCounts,
} from './bench-counts-report.mjs';

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

/** The lines of one measure's table in a report, the header first, found by the summary of its fold. */
function measureTable(report, summary) {
    const start = report.indexOf(`<details><summary>${summary}</summary>`);
    assert.notEqual(start, -1, `no table of ${summary}`);
    return report.slice(start, report.indexOf('</details>', start)).split('\n').filter(line => line.startsWith('|'));
}

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
    assert.deepEqual(measureTable(report, 'Instructions'), ['| Fixture | parse | layout |', '|---|---:|---:|', '| xs | 1,000 | 5,000 |']);
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

test('puts the svg fixture after the others, and its picture load before its layout and PDF write', () => {
    const lines = [
        benchmark('render_doc', 'svg', { ir: 1 }), benchmark('load', 'svg', { ir: 1 }),
        benchmark('parse', 'fonts', { ir: 1 }), benchmark('parse', 'xl', { ir: 1 }),
    ];
    const rows = compare([build('a', lines), build('b', lines)]).filter(r => r.measure === 'instructions');
    assert.deepEqual(rows.map(r => `${r.fixture} ${r.stage}`), ['xl parse', 'fonts parse', 'svg load', 'svg render_doc']);
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
    assert.ok(measureTable(report, 'Instructions').includes('| markdown | 7,000 |'));
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
    assert.deepEqual(measureTable(report, 'Instructions').slice(2), ['| xs | 1,000 |']);
    assert.doesNotMatch(report, /±0\.00%/);
});

test('groups the rows into a table for each measure, the fixtures as rows and the stages as columns', () => {
    const lines = [
        benchmark('end_to_end', 'svg', { ir: 1 }), benchmark('load', 'svg', { ir: 1 }),
        benchmark('data', 'data', { ir: 1 }), benchmark('parse', 'data', { ir: 1 }),
        benchmark('end_to_end', 'xs', { ir: 1 }), benchmark('parse', 'xs', { ir: 1 }),
    ];
    const tables = measureTables(compare([build('a', lines), build('b', lines)]));
    assert.deepEqual(tables.map(t => t.measure), ['instructions', 'estimated_cycles', 'allocations', 'bytes_allocated', 'peak_bytes']);
    for (const t of tables) {
        assert.deepEqual(t.stages, ['load', 'parse', 'data', 'end_to_end']);
        assert.deepEqual(t.fixtures.map(f => f.fixture), ['xs', 'data', 'svg']);
    }
    const svg = tables[0].fixtures[2].cells;
    assert.deepEqual(svg.map(cell => cell?.stage), ['load', undefined, undefined, 'end_to_end']);
    assert.equal(svg[0].measure, 'instructions');
});

test('writes the five tables folded, in the order of the measures, with — for a stage a fixture lacks', () => {
    const lines = [benchmark('parse', 'xs', { ir: 1000 }), benchmark('load', 'svg', { ir: 2000, blocks: 7 })];
    const report = markdown([build('v0.29.0', lines), build('branch', lines)], compare([build('v0.29.0', lines), build('branch', lines)]));
    const summaries = [...report.matchAll(/<details><summary>(.*)<\/summary>/g)].map(m => m[1]);
    assert.deepEqual(summaries, ['Instructions', 'Estimated cycles', 'Allocations', 'Bytes allocated', 'Peak memory']);
    assert.deepEqual(measureTable(report, 'Instructions'), [
        '| Fixture | image load | parse |', '|---|---:|---:|', '| xs | — | 1,000 |', '| svg | 2,000 | — |',
    ]);
    assert.deepEqual(measureTable(report, 'Allocations').slice(2), ['| xs | — | 100 |', '| svg | 7 | — |']);
});

test('with two builds, puts the change under the number where it moved, and the number alone where not', () => {
    const release = build('v0.29.0', [benchmark('parse', 'm', { ir: 1000, blocks: 100 }), benchmark('layout', 'm', { ir: 5000 })]);
    const branch = build('branch', [benchmark('parse', 'm', { ir: 1020, blocks: 95 }), benchmark('layout', 'm', { ir: 5000 })]);
    const report = markdown([release, branch], compare([release, branch]));
    assert.match(report, /A cell is the number of `branch`, and under it, where the number moved, the change from `v0\.29\.0` to `branch`\./);
    assert.deepEqual(measureTable(report, 'Instructions').slice(2), ['| m | 1,020<br>+2.00% | 5,000 |']);
    assert.deepEqual(measureTable(report, 'Allocations').slice(2), ['| m | 95<br>−5.00% | 100 |']);
});

test('marks a change past its threshold in its cell, bold with ⚠', () => {
    const release = build('v0.29.0', [benchmark('end_to_end', 'l', { ir: 1_000_000 })]);
    const branch = build('branch', [benchmark('end_to_end', 'l', { ir: 1_050_000 })]);
    const report = markdown([release, branch], compare([release, branch]));
    assert.deepEqual(measureTable(report, 'Instructions').slice(2), ['| l | 1,050,000<br>**+5.00%** ⚠ |']);
});

test('with three builds, puts both changes in their order where either moved, and says which comes first', () => {
    const builds = [
        build('v0.29.0', [benchmark('parse', 'm', { ir: 1000 }), benchmark('layout', 'm', { ir: 4000 })]),
        build('main', [benchmark('parse', 'm', { ir: 1200 }), benchmark('layout', 'm', { ir: 4000 })]),
        build('branch', [benchmark('parse', 'm', { ir: 1200 }), benchmark('layout', 'm', { ir: 4000 })]),
    ];
    const report = markdown(builds, compare(builds));
    assert.match(report, /where the number moved, the changes from `v0\.29\.0` to `main` first, then from `main` to `branch`\./);
    assert.deepEqual(measureTable(report, 'Instructions').slice(2), ['| m | 1,200<br>**+20.00%** ⚠ · ±0.00% | 4,000 |']);
});

test('with three builds, shows no change from a build that lacks the fixture', () => {
    const builds = [
        build('v0.29.0', [benchmark('parse', 'm', { ir: 1000 })]),
        build('main', [benchmark('parse', 'm', { ir: 1000 }), benchmark('parse', 'markdown', { ir: 7000 })]),
        build('branch', [benchmark('parse', 'm', { ir: 1000 }), benchmark('parse', 'markdown', { ir: 7700 })]),
    ];
    const report = markdown(builds, compare(builds));
    assert.deepEqual(measureTable(report, 'Instructions').slice(2), ['| m | 1,000 |', '| markdown | 7,700<br>— · +10.00% |']);
});

test('writes the builds, the compiler and the thresholds as JSON', () => {
    const lines = [benchmark('parse', 'xs', { ir: 1000 })];
    const builds = [build('v0.29.0', lines), { name: 'branch', counts: build('b', lines).counts }];
    const data = json(builds, compare(builds), { endToEnd: 1, stage: 2, allocations: 3 }, { rust: 'rustc 1.99.0' });
    assert.equal(data.build, 'branch');
    assert.equal(data.rust, 'rustc 1.99.0');
    assert.deepEqual(data.builds, [
        { name: 'v0.29.0', commit: 'v0.29.00000000', short: 'v0.29.0' },
        { name: 'branch', commit: null, short: 'no commit' },
    ]);
    assert.deepEqual(data.thresholds, { endToEnd: 1, stage: 2, allocations: 3 });
});

test('lists each change past its threshold in the JSON, red, with the builds it is between', () => {
    const builds = [
        build('v0.29.0', [benchmark('end_to_end', 'l', { ir: 1_000_000, cycles: 1_000_000 })]),
        build('main', [benchmark('end_to_end', 'l', { ir: 1_000_000, cycles: 1_000_000 })]),
        build('branch', [benchmark('end_to_end', 'l', { ir: 1_050_000, cycles: 1_050_000 })]),
    ];
    const data = json(builds, compare(builds));
    assert.deepEqual(data.marked.map(({ percent, ...m }) => m), ['instructions', 'estimated cycles'].map(measure => ({
        fixture: 'l', stage: 'end to end', measure, before: '1,000,000', after: '1,050,000',
        from: 'main', to: 'branch', text: '+5.00%', marked: true, color: CHANGE_COLORS.marked,
    })));
    assert.equal(Math.round(data.marked[0].percent * 100), 500);
    assert.deepEqual(json(builds.slice(0, 2), compare(builds.slice(0, 2))).marked, []);
});

test('gives each measure a table in the JSON with every stage as a column, in the order of the Markdown', () => {
    const lines = [benchmark('parse', 'xs', { ir: 1000 }), benchmark('load', 'svg', { ir: 2000 })];
    const data = json([build('a', lines), build('b', lines)], compare([build('a', lines), build('b', lines)]));
    assert.deepEqual(data.measures.map(m => m.name), ['Instructions', 'Estimated cycles', 'Allocations', 'Bytes allocated', 'Peak memory']);
    for (const measure of data.measures) {
        assert.deepEqual(measure.stages, STAGES.map(({ id, name }) => ({ id, name })));
        assert.deepEqual(measure.fixtures.map(f => f.name), ['xs', 'svg']);
        assert.ok(measure.fixtures.every(f => f.cells.length === STAGES.length));
    }
    const [xs, svg] = data.measures[0].fixtures;
    assert.deepEqual(xs.cells.map(c => c.text), ['—', '1,000', '—', '—', '—', '—', '—']);
    assert.deepEqual(svg.cells.map(c => c.text), ['2,000', '—', '—', '—', '—', '—', '—']);
});

test('gives a cell the number of each build and each change in the JSON, with a color for each change', () => {
    const builds = [
        build('v0.29.0', [benchmark('parse', 'm', { ir: 1000 })]),
        build('main', [benchmark('parse', 'm', { ir: 1200 })]),
        build('branch', [benchmark('parse', 'm', { ir: 1200 })]),
    ];
    const cell = json(builds, compare(builds)).measures[0].fixtures[0].cells[1];
    assert.equal(cell.value, 1200);
    assert.equal(cell.text, '1,200');
    assert.equal(cell.moved, true);
    assert.deepEqual(cell.values, [
        { build: 'v0.29.0', value: 1000, text: '1,000' },
        { build: 'main', value: 1200, text: '1,200' },
        { build: 'branch', value: 1200, text: '1,200' },
    ]);
    assert.deepEqual(cell.changes, [
        { from: 'v0.29.0', to: 'main', percent: 20, text: '+20.00%', marked: true, color: CHANGE_COLORS.marked },
        { from: 'main', to: 'branch', percent: 0, text: '±0.00%', marked: false, color: CHANGE_COLORS.unmarked },
    ]);
});

test('writes a cell no build has, and a change from a build that lacks the fixture or had zero, with null', () => {
    const builds = [
        build('v0.29.0', [benchmark('parse', 'm', { ir: 1000 })]),
        build('branch', [benchmark('parse', 'm', { ir: 1000 }), benchmark('parse', 'code', { ir: 0, blocks: 0 })]),
        build('next', [benchmark('parse', 'm', { ir: 1000 }), benchmark('parse', 'code', { ir: 10, blocks: 4 })]),
    ];
    const data = json(builds, compare(builds));
    const code = data.measures.find(m => m.id === 'allocations').fixtures.find(f => f.name === 'code').cells;
    assert.deepEqual(code[1].changes.map(c => [c.percent, c.text]), [[null, '—'], [null, 'new']]);
    assert.equal(code[1].moved, true);
    assert.deepEqual(code[0], {
        value: null, text: '—', moved: false,
        values: ['v0.29.0', 'branch', 'next'].map(b => ({ build: b, value: null, text: '—' })),
        changes: [['v0.29.0', 'branch'], ['branch', 'next']].map(([from, to]) => (
            { from, to, percent: null, text: '—', marked: false, color: CHANGE_COLORS.unmarked })),
    });
});

const { marked: RED, unmarked: GRAY } = CHANGE_COLORS;

test('gives each measure one step of two builds, with the end-to-end change of each fixture, red past its threshold', () => {
    const builds = [
        build('v0.29.0', [benchmark('end_to_end', 'xs', { ir: 1000 }), benchmark('end_to_end', 'm', { ir: 2000 })]),
        build('branch', [benchmark('end_to_end', 'xs', { ir: 1010 }), benchmark('end_to_end', 'm', { ir: 2500 })]),
    ];
    const { measures } = json(builds, compare(builds));
    assert.equal(measures.length, 5);
    assert.deepEqual(measures[0].steps, [{
        name: 'v0.29.0 to branch',
        fixtures: [{ name: 'xs', percent: 1, color: GRAY }, { name: 'm', percent: 25, color: RED }],
    }]);
});

test('gives each measure two steps of three builds, in their order, each with its own colors', () => {
    const builds = [
        build('v0.29.0', [benchmark('end_to_end', 'xs', { ir: 1000 }), benchmark('end_to_end', 'm', { ir: 2000 })]),
        build('main', [benchmark('end_to_end', 'xs', { ir: 1250 }), benchmark('end_to_end', 'm', { ir: 2000 })]),
        build('branch', [benchmark('end_to_end', 'xs', { ir: 1250 }), benchmark('end_to_end', 'm', { ir: 2500 })]),
    ];
    const { steps } = json(builds, compare(builds)).measures[0];
    assert.deepEqual(steps.map(s => s.name), ['v0.29.0 to main', 'main to branch']);
    assert.deepEqual(steps.map(s => s.fixtures), [
        [{ name: 'xs', percent: 25, color: RED }, { name: 'm', percent: 0, color: GRAY }],
        [{ name: 'xs', percent: 0, color: GRAY }, { name: 'm', percent: 25, color: RED }],
    ]);
});

test('judges a step of each measure by its own threshold, and charts a drop as a bar to the left', () => {
    const builds = [
        build('v0.29.0', [benchmark('end_to_end', 'xs', { ir: 1000, blocks: 100, bytes: 1000, peak: 1000 })]),
        build('branch', [benchmark('end_to_end', 'xs', { ir: 1050, blocks: 105, bytes: 800, peak: 1000 })]),
    ];
    const bars = json(builds, compare(builds)).measures.map(m => m.steps[0].fixtures[0]);
    assert.deepEqual(bars.map(b => [Math.round(b.percent), b.color]), [[5, RED], [5, RED], [5, GRAY], [-20, GRAY], [0, GRAY]]);
});

test('leaves a fixture out of a step when a build lacks its end-to-end numbers or the change has no percent', () => {
    const builds = [
        build('v0.29.0', [
            benchmark('end_to_end', 'xs', { ir: 1000 }), benchmark('end_to_end', 'justified', { ir: 0, blocks: 0, bytes: 0, peak: 0 }),
            benchmark('parse', 'code', { ir: 500 }),
        ]),
        build('branch', [
            benchmark('end_to_end', 'xs', { ir: 1000 }), benchmark('end_to_end', 'justified', { ir: 10, blocks: 4, bytes: 90, peak: 50 }),
            benchmark('parse', 'code', { ir: 500 }), benchmark('end_to_end', 'chart', { ir: 700 }),
        ]),
    ];
    for (const { steps } of json(builds, compare(builds)).measures) {
        assert.deepEqual(steps[0].fixtures, [{ name: 'xs', percent: 0, color: GRAY }]);
    }
});

test('gives a step no fixtures when no fixture has end-to-end numbers in both builds, and a single build no steps', () => {
    const lines = [benchmark('parse', 'xs', { ir: 1000 })];
    const builds = [build('v0.29.0', lines), build('branch', lines)];
    assert.deepEqual(json(builds, compare(builds)).measures[0].steps, [{ name: 'v0.29.0 to branch', fixtures: [] }]);
    assert.deepEqual(json(builds.slice(1), compare(builds.slice(1))).measures[0].steps, []);
});

/**
 * Renders the JSON of `builds` with the template and the command line in LPDF_CLI, and returns the PDF's text,
 * read as Latin-1 to count its pages.
 */
function renderPdf(builds) {
    const folder = mkdtempSync(path.join(tmpdir(), 'bench-counts-report-'));
    try {
        const data = path.join(folder, 'report.json');
        const pdf = path.join(folder, 'report.pdf');
        writeFileSync(data, JSON.stringify(json(builds, compare(builds), undefined, { rust: 'rustc 1.99.0' })));
        const template = path.join(path.dirname(fileURLToPath(import.meta.url)), 'bench-counts-report.xml');
        execFileSync(process.env.LPDF_CLI, ['convert', '--input', template, '--data', data, '--output', pdf], { stdio: 'pipe' });
        return readFileSync(pdf, 'latin1');
    } finally {
        rmSync(folder, { recursive: true, force: true });
    }
}

/** A run's benchmarks: two sizes, the data binding and the image load, and fixtures of features. */
const run = scale => [
    benchmark('parse', 'xs', { ir: 1000 * scale }), benchmark('end_to_end', 'xs', { ir: 5000 * scale }),
    benchmark('parse', 'xl', { ir: 9000 }), benchmark('end_to_end', 'xl', { ir: 40_000 }),
    benchmark('parse', 'data', { ir: 3000 }), benchmark('data', 'data', { ir: 2000 * scale }),
    benchmark('load', 'svg', { ir: 7000, peak: 4000 * scale }), benchmark('render_doc', 'svg', { ir: 8000 }),
];

const pages = pdf => pdf.match(/\/Type\s*\/Page\b/g).length;

// A cover and a page for each measure at least: how many pages the cover takes is the engine's layout to decide.
test('renders the JSON of two builds with the template, with changes over the thresholds', { skip: !process.env.LPDF_CLI }, () => {
    const pdf = renderPdf([build('v0.29.0', run(1)), build('branch', [...run(1.2), benchmark('parse', 'code', { ir: 100 })])]);
    assert.ok(pages(pdf) >= 6, `${pages(pdf)} pages`);
});

test('renders the JSON of three builds with the template, with nothing over the thresholds', { skip: !process.env.LPDF_CLI }, () => {
    const pdf = renderPdf([build('v0.29.0', run(1)), build('main', run(1)), build('branch', run(1))]);
    assert.ok(pages(pdf) >= 6, `${pages(pdf)} pages`);
});

test('gives the template a column for the fixture and one for each stage', () => {
    // The measure table's `cols` are written out, and a table drops a cell it has no column for: a stage added to
    // STAGES without a column would lose the last one, end to end, from the PDF.
    const template = readFileSync(path.join(path.dirname(fileURLToPath(import.meta.url)), 'bench-counts-report.xml'), 'utf8');
    const stages = template.indexOf('data-source="stages"');
    const cols = [...template.slice(0, stages).matchAll(/<table cols="([^"]*)"/g)].at(-1)[1];
    assert.equal(cols.trim().split(/\s+/).length, STAGES.length + 1);
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
