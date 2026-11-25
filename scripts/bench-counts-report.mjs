// The tables of bench.yml: the instructions, estimated cycles and allocations that benches/counts measured
// in two or three builds, the last release first, and the change from each build to the next, with the
// changes over a threshold marked. The marked rows come first, then a table for each measure, with the
// fixtures as rows and the stages as columns. Report only: it never fails on what the numbers show.
//
// Each build is a folder bench.yml writes (scripts/bench-counts.sh):
//   build.json          {"name": "v0.29.0", "commit": "<sha>"}
//   summary.jsonl       instructions and estimated cycles: the output of `cargo bench --bench counts --
//                       --output-format=json` in benches/counts, a line a benchmark
//   allocations.jsonl   allocations, bytes allocated and peak memory: the output of `cargo bench --bench
//                       allocations`, a line a benchmark
//
// Usage: node scripts/bench-counts-report.mjs [options] <release> [<main>] <build>
//   --mark-end-to-end <percent>   instructions or estimated cycles of a whole render (default 3)
//   --mark-stage <percent>        instructions or estimated cycles of one stage (default 10)
//   --mark-allocations <percent>  allocations, bytes allocated or peak memory of any stage (default 10)
//   --markdown <file>             write the report there instead of to stdout
//   --json <file>                 write the report as JSON there too: the data that
//                                 scripts/bench-counts-report.xml renders to a PDF
//   --history <file>              write the counts of the last build as JSON, with the build before it as
//                                 `against`: what lpdf-bench keeps as counts/vX.Y.Z.json
//   --about <key>=<value>         added to the history, such as rust=<rustc -V> (repeatable)

import { readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

/** The stages of benches/counts, in pipeline order, by the name of their benchmark function. */
export const STAGES = [
    // The fixture `svg` only: its pictures loaded, each SVG read and written as PDF content.
    { id: 'load', name: 'image load' },
    { id: 'parse', name: 'parse' },
    // The fixture `data` only: its data bound to the parsed document.
    { id: 'data', name: 'data binding' },
    { id: 'layout', name: 'layout' },
    { id: 'write_pdf', name: 'PDF write' },
    { id: 'render_doc', name: 'layout + PDF write' },
    { id: 'end_to_end', name: 'end to end' },
];

/** Where the end-to-end stage is in a cell list of the JSON, which has a cell for every stage of `STAGES`. */
const END_TO_END = STAGES.findIndex(s => s.id === 'end_to_end');

/** The fixtures, `bench_xs` to `bench_xl`, by the id of their benchmark. */
export const FIXTURES = ['xs', 's', 'm', 'l', 'xl'];

/** The fixtures of one feature each, in the order of the table, after the sizes. */
export const FEATURE_FIXTURES = ['justified', 'data', 'markdown', 'code', 'chart', 'chart_lines', 'fonts', 'svg'];

/**
 * What the table shows. The work is Callgrind's, under the metric of gungraun's summary; the memory is the
 * counting allocator's, under the same id in its lines.
 */
export const MEASURES = [
    { id: 'instructions', name: 'instructions', metric: 'Ir', kind: 'work' },
    { id: 'estimated_cycles', name: 'estimated cycles', metric: 'EstimatedCycles', kind: 'work' },
    { id: 'allocations', name: 'allocations', kind: 'memory' },
    { id: 'bytes_allocated', name: 'bytes allocated', kind: 'memory' },
    // The most the heap grew over where it stood when the stage began.
    { id: 'peak_bytes', name: 'peak memory', kind: 'memory' },
];

export const DEFAULT_MARKS = { endToEnd: 3, stage: 10, allocations: 10 };

const lines = text => text.split('\n').filter(line => line.trim()).map(line => JSON.parse(line));

/**
 * The counts of one build, `fixture → stage → measure → number`, from the lines of its two files: gungraun's
 * summary for the work, the counting allocator's for the memory. A benchmark that lacks a measure, or is in
 * one file and not the other, is an error: the table would show a gap as if the stage were new.
 */
export function readCounts(summaryText, allocationsText) {
    const counts = {};
    const work = MEASURES.filter(m => m.kind === 'work');
    for (const bench of lines(summaryText)) {
        const callgrind = bench.profiles.find(p => p.tool === 'Callgrind')?.data.total.metrics;
        const measures = {};
        for (const measure of work) {
            const value = callgrind?.[measure.metric]?.values?.new;
            if (typeof value !== 'number') {
                throw new Error(`${bench.function_name}.${bench.id} has no Callgrind ${measure.metric}`);
            }
            measures[measure.id] = value;
        }
        counts[bench.id] ??= {};
        counts[bench.id][bench.function_name] = measures;
    }
    const memory = MEASURES.filter(m => m.kind === 'memory');
    for (const bench of lines(allocationsText)) {
        const measures = counts[bench.fixture]?.[bench.stage];
        if (!measures) throw new Error(`${bench.stage}.${bench.fixture} has allocations but no instructions`);
        for (const measure of memory) {
            if (typeof bench[measure.id] !== 'number') throw new Error(`${bench.stage}.${bench.fixture} has no ${measure.id}`);
            measures[measure.id] = bench[measure.id];
        }
    }
    for (const [fixture, stages] of Object.entries(counts)) {
        for (const [stage, measures] of Object.entries(stages)) {
            if (!('allocations' in measures)) throw new Error(`${stage}.${fixture} has instructions but no allocations`);
        }
    }
    return counts;
}

/** The mark of a row in percent: how much it may grow before the table marks it. */
export function markFor(stage, measure, marks) {
    if (measure.kind === 'memory') return marks.allocations;
    return stage === 'end_to_end' ? marks.endToEnd : marks.stage;
}

/** Percent from `before` to `after`, or undefined when either build lacks the row. */
export function change(before, after) {
    if (before === undefined || after === undefined) return undefined;
    if (before === 0) return after === 0 ? 0 : Infinity;
    return (after - before) / before * 100;
}

const byOrder = order => (a, b) => {
    const ia = order.indexOf(a), ib = order.indexOf(b);
    return (ia < 0 ? order.length : ia) - (ib < 0 ? order.length : ib) || a.localeCompare(b);
};

/**
 * The rows of the table, one per fixture, stage and measure that any build has: the value of each build,
 * and the change from each build to the next, with `marked` set where it grew past its mark.
 */
export function compare(builds, marks = DEFAULT_MARKS) {
    const fixtures = [...new Set(builds.flatMap(b => Object.keys(b.counts)))].sort(byOrder([...FIXTURES, ...FEATURE_FIXTURES]));
    const stageIds = STAGES.map(s => s.id);
    const rows = [];
    for (const fixture of fixtures) {
        const stages = [...new Set(builds.flatMap(b => Object.keys(b.counts[fixture] ?? {})))].sort(byOrder(stageIds));
        for (const stage of stages) {
            for (const measure of MEASURES) {
                const values = builds.map(b => b.counts[fixture]?.[stage]?.[measure.id]);
                const mark = markFor(stage, measure, marks);
                const changes = values.slice(1).map((value, i) => {
                    const percent = change(values[i], value);
                    return { percent, marked: percent !== undefined && percent > mark };
                });
                rows.push({ fixture, stage, measure: measure.id, values, changes, mark });
            }
        }
    }
    return rows;
}

/** The stages any row has, in pipeline order. */
const stagesOf = rows => [...new Set(rows.map(r => r.stage))].sort(byOrder(STAGES.map(s => s.id)));

/**
 * The rows of `compare` as a table for each measure, in the order of `MEASURES`: the fixtures as rows, in the
 * order of `compare`, and as columns the stages any fixture has, in pipeline order, or the `stages` given. A
 * cell is the row of that fixture, stage and measure, or undefined where the fixture has no such stage.
 */
export function measureTables(rows, stages = stagesOf(rows)) {
    const fixtures = [...new Set(rows.map(r => r.fixture))];
    const byKey = new Map(rows.map(r => [`${r.measure} ${r.fixture} ${r.stage}`, r]));
    return MEASURES.map(measure => ({
        measure: measure.id,
        stages,
        fixtures: fixtures.map(fixture => ({
            fixture,
            cells: stages.map(stage => byKey.get(`${measure.id} ${fixture} ${stage}`)),
        })),
    }));
}

const stageName = id => STAGES.find(s => s.id === id)?.name ?? id;
const measureName = id => MEASURES.find(m => m.id === id)?.name ?? id;
const count = n => n === undefined ? '—' : n.toLocaleString('en-US');

/** A change as text: `+1.20%`, `−0.50%`, `±0.00%`, `new` from zero, or `—` where a build lacks the row. */
function percentText(delta) {
    if (delta.percent === undefined) return '—';
    if (delta.percent === Infinity) return 'new';
    const sign = delta.percent > 0 ? '+' : delta.percent < 0 ? '−' : '±';
    return `${sign}${Math.abs(delta.percent).toFixed(2)}%`;
}

function percentCell(delta) {
    const text = percentText(delta);
    return delta.marked ? `**${text}** ⚠` : text;
}

/** Whether any change of a row isn't zero: a build lacking the row is no change. */
const moved = row => row.changes.some(c => c.percent !== undefined && c.percent !== 0);

/** The headers of the change columns: from each build to the next. */
const changeHeaders = builds => builds.slice(1).map((b, i) => `${builds[i].name} → ${b.name}`);

function table(rows, builds) {
    const headers = ['Fixture', 'Stage', 'Measure', ...builds.map(b => b.name), ...changeHeaders(builds)];
    const lines = [
        `| ${headers.join(' | ')} |`,
        `|${headers.map((_, i) => i < 3 ? '---' : '---:').join('|')}|`,
    ];
    for (const row of rows) {
        const cells = [
            row.fixture, stageName(row.stage), measureName(row.measure),
            ...row.values.map(count), ...row.changes.map(percentCell),
        ];
        lines.push(`| ${cells.join(' | ')} |`);
    }
    return lines.join('\n');
}

const capitalized = text => text.charAt(0).toUpperCase() + text.slice(1);

/**
 * A cell of a measure's table: the last build's number, and under it, when any change from build to build
 * isn't zero, every change in order, so with three builds each keeps its place.
 */
function measureCell(row) {
    const value = row?.values[row.values.length - 1];
    if (value === undefined) return '—';
    return moved(row) ? `${count(value)}<br>${row.changes.map(percentCell).join(' · ')}` : count(value);
}

function measureTable({ stages, fixtures }) {
    const headers = ['Fixture', ...stages.map(stageName)];
    const lines = [
        `| ${headers.join(' | ')} |`,
        `|${headers.map((_, i) => i < 1 ? '---' : '---:').join('|')}|`,
    ];
    for (const { fixture, cells } of fixtures) {
        lines.push(`| ${[fixture, ...cells.map(measureCell)].join(' | ')} |`);
    }
    return lines.join('\n');
}

/** What a cell of the measure tables holds, and with three builds, which change comes first. */
function cellNote(builds) {
    const names = builds.map(b => `\`${b.name}\``);
    const last = names[names.length - 1];
    const steps = names.slice(1).map((name, i) => `from ${names[i]} to ${name}`);
    const changes = steps.length === 1 ? `the change ${steps[0]}` : `the changes ${steps[0]} first, then ${steps[1]}`;
    return `A table for each measure, a row for each fixture and a column for each stage. A cell is the number of ${last}, ` +
        `and under it, where the number moved, ${changes}. A cell reads — where the fixture has no such stage.`;
}

/** The stages a build lacks that another has, as `{ build, stages }`, so the table can say why a column is empty. */
export function missingStages(builds) {
    const all = new Set(builds.flatMap(b => Object.values(b.counts).flatMap(Object.keys)));
    return builds
        .map(b => {
            const own = new Set(Object.values(b.counts).flatMap(Object.keys));
            return { build: b.name, stages: [...all].filter(s => !own.has(s)).sort(byOrder(STAGES.map(s => s.id))) };
        })
        .filter(m => m.stages.length > 0);
}

/** `a`, `a or b`, `a, b or c`. */
const orList = items => items.length < 2 ? items.join('') : `${items.slice(0, -1).join(', ')} or ${items[items.length - 1]}`;

/**
 * The fixtures a build lacks that another has, as `{ build, fixtures }`: the release before a feature has no
 * fixture of it, so its rows have no baseline until the next release.
 */
export function missingFixtures(builds) {
    const all = new Set(builds.flatMap(b => Object.keys(b.counts)));
    return builds
        .map(b => ({ build: b.name, fixtures: [...all].filter(f => !(f in b.counts)).sort(byOrder(FEATURE_FIXTURES)) }))
        .filter(m => m.fixtures.length > 0);
}

/** The report in Markdown: what was compared, the marked rows, and a folded table for each measure. */
export function markdown(builds, rows, marks = DEFAULT_MARKS, about = {}) {
    const marked = rows.filter(r => r.changes.some(c => c.marked));
    const built = builds.map(b => `\`${b.name}\` (${b.commit ? b.commit.slice(0, 7) : 'no commit'})`).join(', ');
    const out = [
        '### Instruction and allocation counts',
        '',
        `Builds: ${built}${about.rust ? `, all with ${about.rust}` : ''}.`,
        'Instructions and estimated cycles from Callgrind; allocations, bytes allocated and peak memory from a counting allocator. ' +
            'The peak is the most the heap grew over where it stood when the stage began.',
        `Marked (⚠): instructions or estimated cycles up more than ${marks.endToEnd}% end to end or ${marks.stage}% in one stage, ` +
            `allocations, bytes allocated or peak memory up more than ${marks.allocations}%. Report only: the run passes whatever it shows.`,
        '',
    ];
    for (const { build, stages } of missingStages(builds)) {
        out.push(`\`${build}\` has no ${stages.map(s => stageName(s)).join(' or ')} benchmark, so those rows have no number for it.`, '');
    }
    for (const { build, fixtures } of missingFixtures(builds)) {
        const named = orList(fixtures.map(f => `\`${f}\``));
        out.push(`\`${build}\` has no ${named} fixture, so those rows have no baseline: they compare from the first release that has them.`, '');
    }
    if (marked.length === 0) {
        out.push('**Nothing over the thresholds.**', '');
    } else {
        out.push(`**${marked.length} over the thresholds:**`, '', table(marked, builds), '');
    }
    out.push(cellNote(builds), '');
    for (const grid of measureTables(rows)) {
        const summary = capitalized(measureName(grid.measure));
        out.push(`<details><summary>${summary}</summary>`, '', measureTable(grid), '', '</details>', '');
    }
    return out.join('\n');
}

/** The color of a change in the JSON: red past its threshold, gray otherwise. */
export const CHANGE_COLORS = { marked: '#c0392b', unmarked: '#737373' };

/** A time as the release notes PDF of release.yml writes it, in UTC to the minute: `2026-11-25 20:06 UTC`. */
export const utcTime = date => `${date.toISOString().slice(0, 16).replace('T', ' ')} UTC`;

/**
 * The report as the data of the PDF (scripts/bench-counts-report.xml): the builds, the thresholds, each change past
 * them, and a table for each measure, as the Markdown has them. A table has every stage of `STAGES` as a column,
 * since the template's table has a fixed number of columns, and a cell has the number of each build and each
 * change from build to build, in their order. Numbers come as they are, `null` where a build lacks them, and as
 * text, and each change has its color. A measure has `steps` too, what its charts show: a step for each change from
 * a build to the next, named from the two builds, with each fixture's end-to-end change and its color. A fixture
 * whose build lacks the end-to-end numbers, or whose change has no percent, is left out of the step, so it has no bar.
 * `generated` is the time the report was made, from `about`, as the cover says it; `null` without one.
 */
export function json(builds, rows, marks = DEFAULT_MARKS, about = {}) {
    const changeData = (delta, i) => ({
        from: builds[i].name,
        to: builds[i + 1].name,
        percent: Number.isFinite(delta.percent) ? delta.percent : null,
        text: percentText(delta),
        marked: delta.marked,
        color: delta.marked ? CHANGE_COLORS.marked : CHANGE_COLORS.unmarked,
    });
    const noRow = { values: builds.map(() => undefined), changes: builds.slice(1).map(() => ({ percent: undefined, marked: false })) };
    const cellData = (row = noRow) => {
        const value = row.values[row.values.length - 1];
        return {
            value: value ?? null,
            text: count(value),
            moved: moved(row),
            values: row.values.map((v, i) => ({ build: builds[i].name, value: v ?? null, text: count(v) })),
            changes: row.changes.map(changeData),
        };
    };
    const stepsOf = ({ fixtures }) => builds.slice(1).map((to, i) => ({
        name: `${builds[i].name} to ${to.name}`,
        fixtures: fixtures.flatMap(({ fixture, cells }) => {
            const delta = cells[END_TO_END]?.changes[i];
            const { percent, color } = delta ? changeData(delta, i) : {};
            return typeof percent === 'number' ? [{ name: fixture, percent, color }] : [];
        }),
    }));
    const marked = rows.flatMap(row => row.changes.flatMap((delta, i) => delta.marked
        ? [{
            fixture: row.fixture,
            stage: stageName(row.stage),
            measure: measureName(row.measure),
            before: count(row.values[i]),
            after: count(row.values[i + 1]),
            ...changeData(delta, i),
        }]
        : []));
    return {
        build: builds[builds.length - 1].name,
        rust: about.rust ?? null,
        generated: about.generated ?? null,
        builds: builds.map(b => ({ name: b.name, commit: b.commit ?? null, short: b.commit ? b.commit.slice(0, 7) : 'no commit' })),
        thresholds: { endToEnd: marks.endToEnd, stage: marks.stage, allocations: marks.allocations },
        marked,
        measures: measureTables(rows, STAGES.map(s => s.id)).map(grid => ({
            id: grid.measure,
            name: capitalized(measureName(grid.measure)),
            stages: grid.stages.map(id => ({ id, name: stageName(id) })),
            fixtures: grid.fixtures.map(({ fixture, cells }) => ({ name: fixture, cells: cells.map(cellData) })),
            steps: stepsOf(grid),
        })),
    };
}

/** What lpdf-bench keeps for a release: its counts, and those of the build before it, measured in the same run. */
export function history(builds, about = {}) {
    const last = builds[builds.length - 1];
    const before = builds[builds.length - 2];
    return {
        version: last.name,
        commit: last.commit,
        ...about,
        counts: last.counts,
        against: before ? { version: before.name, commit: before.commit, counts: before.counts } : null,
    };
}

/** A build folder of bench.yml: its name, commit and counts. */
export function readBuild(folder) {
    const read = file => readFileSync(path.join(folder, file), 'utf8');
    const build = JSON.parse(read('build.json'));
    return { name: build.name, commit: build.commit, counts: readCounts(read('summary.jsonl'), read('allocations.jsonl')) };
}

function usage(message) {
    console.error(`bench-counts-report: ${message}`);
    console.error('usage: node scripts/bench-counts-report.mjs [--mark-end-to-end <%>] [--mark-stage <%>] [--mark-allocations <%>]');
    console.error('           [--markdown <file>] [--json <file>] [--history <file>] [--about <key>=<value>]... <release> [<main>] <build>');
    process.exit(2);
}

function main(args) {
    const marks = { ...DEFAULT_MARKS };
    const about = {};
    const folders = [];
    let markdownFile, jsonFile, historyFile;
    const percentArg = (name, value) => {
        const n = Number(value);
        if (value === undefined || !Number.isFinite(n) || n < 0) usage(`${name} takes a percentage, not '${value}'`);
        return n;
    };
    for (let i = 0; i < args.length; i++) {
        const arg = args[i];
        switch (arg) {
            case '--mark-end-to-end': marks.endToEnd = percentArg(arg, args[++i]); break;
            case '--mark-stage': marks.stage = percentArg(arg, args[++i]); break;
            case '--mark-allocations': marks.allocations = percentArg(arg, args[++i]); break;
            case '--markdown': markdownFile = args[++i]; break;
            case '--json': jsonFile = args[++i]; break;
            case '--history': historyFile = args[++i]; break;
            case '--about': {
                const [key, ...value] = (args[++i] ?? '').split('=');
                if (!key || value.length === 0) usage('--about takes <key>=<value>');
                about[key] = value.join('=');
                break;
            }
            default:
                if (arg.startsWith('--')) usage(`unknown option ${arg}`);
                folders.push(arg);
        }
    }
    if (folders.length < 2 || folders.length > 3) usage('give two or three build folders: the release, main if any, and the build');

    const builds = folders.map(readBuild);
    const rows = compare(builds, marks);
    const report = markdown(builds, rows, marks, about);
    if (markdownFile) writeFileSync(markdownFile, report);
    else process.stdout.write(report);
    // The time goes in the PDF's data only: the history lpdf-bench keeps is the counts, and a re-run of a job has
    // the same counts at another time.
    const generated = utcTime(new Date());
    if (jsonFile) writeFileSync(jsonFile, `${JSON.stringify(json(builds, rows, marks, { ...about, generated }), null, 2)}\n`);
    if (historyFile) writeFileSync(historyFile, `${JSON.stringify(history(builds, about), null, 2)}\n`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    main(process.argv.slice(2));
}
