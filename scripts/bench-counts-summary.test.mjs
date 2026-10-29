// Tests of bench-counts-summary.mjs: `node --test scripts/`. bench.yml runs them before it builds anything.

import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { test } from 'node:test';

import { compareVersions, markdown, readReleases, step, summarize } from './bench-counts-summary.mjs';

/** The counts of one stage of one fixture, with only what the table reads. */
const stage = (instructions, extra = {}) => ({ instructions, allocations: 100, bytes_allocated: 1_000_000, peak_bytes: 500_000, ...extra });

/** The counts of a build: the instructions of each stage, the same for every fixture, and what a whole render allocates. */
const counts = (instructions, extra = {}) => Object.fromEntries(['xs', 's', 'm', 'l', 'xl'].map(fixture => [
    fixture,
    Object.fromEntries(Object.entries(instructions).map(([name, ir]) => [name, stage(ir, extra)])),
]));

/** A release as lpdf-bench keeps it, measured against the build `before`. */
const release = (version, instructions, before, extra = {}) => ({
    version,
    commit: `${version}0000000`,
    rust: 'rustc 1.99.0 (b940084d7 2026-09-28)',
    counts: counts(instructions, extra),
    against: before && { version: before.version, commit: before.commit, counts: counts(before.instructions) },
});

/** Fills `against` of a release from the instructions of the release before it. */
function chain(...steps) {
    let before;
    return steps.map(([version, instructions, extra]) => {
        const r = release(version, instructions, before ?? { version: 'v0.23.0', commit: 'v0.23.00000000', instructions }, extra);
        before = { version, commit: r.commit, instructions };
        return r;
    });
}

/** The cells of a release's row in the first table, or with `peak` in the table of the peak of each stage. */
function rowOf(report, version, { peak = false } = {}) {
    const lines = report.split('\n');
    const from = peak ? lines.indexOf('## Peak memory of each stage') : 0;
    return lines.slice(from).find(line => line.startsWith(`| ${version} |`)).split('|').slice(1, -1).map(c => c.trim());
}

test('orders v0.10.0 after v0.9.0', () => {
    assert.deepEqual(['v0.10.0', 'v0.9.1', 'v0.9.0', 'v1.0.0'].sort(compareVersions), ['v0.9.0', 'v0.9.1', 'v0.10.0', 'v1.0.0']);
});

test('reads the releases of a folder oldest first and skips the files that are not releases', () => {
    const folder = mkdtempSync(path.join(tmpdir(), 'bench-counts-summary-'));
    try {
        for (const version of ['v0.10.0', 'v0.9.0', 'v0.2.0']) {
            writeFileSync(path.join(folder, `${version}.json`), JSON.stringify({ version }));
        }
        writeFileSync(path.join(folder, 'README.md'), '# Counts');
        writeFileSync(path.join(folder, 'SUMMARY.md'), '# Counts of each release');
        assert.deepEqual(readReleases(folder).map(r => r.version), ['v0.2.0', 'v0.9.0', 'v0.10.0']);
    } finally {
        rmSync(folder, { recursive: true });
    }
});

test('takes the step as the geometric mean of the fixtures, so each counts the same whatever its size', () => {
    const r = release('v0.2.0', { parse: 1 }, { version: 'v0.1.0', commit: 'v0.1.00000000', instructions: { parse: 1 } });
    r.counts.xs.parse.instructions = 400;
    r.against.counts.xs.parse.instructions = 100;
    r.counts.xl.parse.instructions = 2_000_000_000;
    r.against.counts.xl.parse.instructions = 2_000_000_000;
    // Ratios 4, 1, 1, 1, 1: the fifth root of 4 is 1.3195
    assert.equal(step(r, 'parse').toFixed(4), '1.3195');
});

test('has no step for a stage that the release before lacks', () => {
    const r = release('v0.2.0', { parse: 10, layout: 20 }, { version: 'v0.1.0', commit: 'v0.1.00000000', instructions: { parse: 10 } });
    assert.equal(step(r, 'parse'), 1);
    assert.equal(step(r, 'layout'), undefined);
});

test('starts every stage at 100 and multiplies the index of the release before by each step', () => {
    const rows = summarize(chain(
        ['v0.9.0', { parse: 100, end_to_end: 1000 }],
        ['v0.10.0', { parse: 110, end_to_end: 1000 }],
        ['v0.11.0', { parse: 121, end_to_end: 1010 }],
    ));
    assert.deepEqual(rows.map(r => r.stages.parse.index.toFixed(1)), ['100.0', '110.0', '121.0']);
    assert.deepEqual(rows.map(r => r.stages.end_to_end.index.toFixed(1)), ['100.0', '100.0', '101.0']);
    assert.deepEqual(rows.map(r => r.gap), [undefined, undefined, undefined]);
});

test('shows the step of the first row from its own release before, which the folder lacks', () => {
    const [first] = summarize([release('v0.24.0', { parse: 105 }, { version: 'v0.23.0', commit: 'v0.23.00000000', instructions: { parse: 100 } })]);
    assert.equal(first.stages.parse.index, 100);
    assert.equal(first.stages.parse.step.toFixed(2), '1.05');
    assert.equal(rowOf(markdown([first]), 'v0.24.0')[1], '100.0 (+5.0%)');
});

test('starts a stage at 100 without a step where the release before lacks it, and shows — before that', () => {
    const rows = summarize(chain(
        ['v0.9.0', { parse: 100, render_doc: 500 }],
        ['v0.10.0', { parse: 100, render_doc: 500, layout: 300, write_pdf: 200 }],
        ['v0.11.0', { parse: 100, render_doc: 500, layout: 330, write_pdf: 200 }],
    ));
    const report = markdown(rows);
    // Release, Parse, Layout, PDF write, Layout + PDF write
    assert.deepEqual(rowOf(report, 'v0.9.0').slice(1, 5), ['100.0 (±0.0%)', '—', '—', '100.0 (±0.0%)']);
    assert.deepEqual(rowOf(report, 'v0.10.0').slice(1, 5), ['100.0 (±0.0%)', '100.0', '100.0', '100.0 (±0.0%)']);
    assert.deepEqual(rowOf(report, 'v0.11.0').slice(1, 5), ['100.0 (±0.0%)', '110.0 (+10.0%)', '100.0 (±0.0%)', '100.0 (±0.0%)']);
});

test('writes a step down with a minus sign', () => {
    const report = markdown(summarize(chain(['v0.1.0', { parse: 1000 }], ['v0.2.0', { parse: 900 }])));
    assert.equal(rowOf(report, 'v0.2.0')[1], '90.0 (−10.0%)');
});

test('shows allocations as counts and bytes in MB with two decimals, end to end, for m and xl', () => {
    const r = release('v0.29.0', { end_to_end: 1000 }, undefined);
    r.counts.m.end_to_end = { instructions: 1000, allocations: 5044, bytes_allocated: 6_765_432, peak_bytes: 1_234_567 };
    r.counts.xl.end_to_end = { instructions: 1000, allocations: 16888, bytes_allocated: 26_260_000, peak_bytes: 9_876_543 };
    const cells = rowOf(markdown(summarize([r])), 'v0.29.0');
    assert.deepEqual(cells.slice(6, 8), ['5,044 / 16,888', '6.77 / 26.26']);
});

test('shows the peak memory of each stage for m and xl in a table of its own, with — for a stage the release lacks', () => {
    const r = release('v0.30.0', { parse: 100, layout: 300, write_pdf: 200, render_doc: 500, end_to_end: 600 }, undefined);
    r.counts.m.parse.peak_bytes = 1_179_871;
    r.counts.xl.parse.peak_bytes = 7_386_425;
    r.counts.m.write_pdf.peak_bytes = 546_206;
    r.counts.xl.write_pdf.peak_bytes = 1_199_935;
    const old = release('v0.29.0', { parse: 100, render_doc: 500, end_to_end: 600 }, undefined);
    const report = markdown(summarize([old, r]));
    assert.match(report, /## Peak memory of each stage/);
    assert.deepEqual(rowOf(report, 'v0.30.0', { peak: true }), ['v0.30.0', '1.18 / 7.39', '0.50 / 0.50', '0.55 / 1.20', '0.50 / 0.50', '0.50 / 0.50']);
    assert.deepEqual(rowOf(report, 'v0.29.0', { peak: true }), ['v0.29.0', '0.50 / 0.50', '—', '—', '0.50 / 0.50', '0.50 / 0.50']);
});

test('shows — for the peak memory of a release saved without it', () => {
    const r = release('v0.24.0', { end_to_end: 1000 }, undefined);
    for (const fixture of Object.values(r.counts)) delete fixture.end_to_end.peak_bytes;
    const report = markdown(summarize([r]));
    assert.deepEqual(rowOf(report, 'v0.24.0').slice(6, 8), ['100 / 100', '1.00 / 1.00']);
    assert.deepEqual(rowOf(report, 'v0.24.0', { peak: true }), ['v0.24.0', '—', '—', '—', '—', '—']);
});

test('shows a release with an error as not measured, and goes on from the release before it', () => {
    const measured = chain(['v0.1.0', { parse: 100 }], ['v0.3.0', { parse: 110 }]);
    measured[1].against.version = 'v0.2.0';
    const broken = { version: 'v0.2.0', commit: 'v0.2.00000000', rust: 'rustc 1.99.0 (b940084d7 2026-09-28)', error: 'v0.2.0 did not build', counts: null };
    const rows = summarize([measured[0], broken, measured[1]]);
    assert.equal(rows[1].stages, null);
    assert.equal(rows[2].stages.parse.index.toFixed(1), '110.0');
    assert.equal(rows[2].gap, 'v0.2.0');

    const report = markdown(rows);
    assert.deepEqual(rowOf(report, 'v0.2.0'), ['v0.2.0', 'not measured', '—', '—', '—', '—', '—', '—', '1.99.0']);
    assert.deepEqual(rowOf(report, 'v0.2.0', { peak: true }), ['v0.2.0', 'not measured', '—', '—', '—', '—']);
    assert.match(report, /The step of `v0\.3\.0` is from `v0\.2\.0` that has no counts here: the index goes on from `v0\.1\.0`/);
});

test('says how the index is made and that the file is generated', () => {
    const report = markdown(summarize(chain(['v0.1.0', { parse: 100 }])));
    assert.match(report, /^# Counts of each release/);
    assert.match(report, /don't edit it/);
    assert.match(report, /geometric mean/);
    assert.match(report, /MB = 1,000,000 bytes/);
    assert.match(report, /the peak memory from a counting allocator/);
    assert.match(report, /fixtures in 0\.27/);
    assert.match(report, /The first row shows its step from v0\.23\.0/);
});

test('names the Rust of each release by its version', () => {
    const r = release('v0.24.0', { parse: 100 }, undefined);
    assert.equal(rowOf(markdown(summarize([r])), 'v0.24.0').at(-1), '1.99.0');
    delete r.rust;
    assert.equal(rowOf(markdown(summarize([r])), 'v0.24.0').at(-1), '—');
});
