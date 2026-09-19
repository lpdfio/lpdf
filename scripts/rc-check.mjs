#!/usr/bin/env node
// Shows where a core release candidate stands (Lpdf-devcycle.md §4.2): core's
// release.yml run for the RC tag, then the rc.yml run it dispatched in each
// of the five SDK and extension repos, and whether the RC is ready to release.
//
//   make rc-check                   the newest RC tag on GitHub
//   make rc-check RC=v0.19.0-rc.2   a specific one
//
// Read-only: it pushes nothing and changes nothing. The repos are public, so
// no token is needed, but GitHub allows 60 unauthenticated API calls an hour.
// A check uses about 6 while runs are going and up to ~20 once they finish.
// Set GH_TOKEN or GITHUB_TOKEN to raise the limit to 5,000.

import { execFileSync } from 'node:child_process'

const OWNER = 'lpdfio'
const CORE = 'lpdf'
const REPOS = ['lpdf-js', 'lpdf-dotnet', 'lpdf-php', 'lpdf-python', 'lpdf-vscode']
const REMOTE = 'origin'
const BRANCH = 'main'
const API = process.env.LPDF_GITHUB_API || 'https://api.github.com' // overridable for tests
const WEB = 'https://github.com'
const TOKEN = process.env.GH_TOKEN || process.env.GITHUB_TOKEN || ''
// A dispatched run appears within seconds. After this long with no run, the
// repo most likely has no rc.yml on main, or the dispatch failed.
const MISSING_AFTER_MS = 3 * 60 * 1000

function fail(message) {
    console.error(`rc-check: ${message}`)
    process.exit(1)
}

// ── Tags and main, from GitHub (local tags can be stale) ──────────────────────

function lsRemote(...args) {
    try {
        return execFileSync('git', ['ls-remote', ...args], { encoding: 'utf8' })
    } catch {
        fail(`could not read ${REMOTE} (git ls-remote)`)
    }
}

// Tag name → commit. Annotated tags list twice; the peeled `^{}` line is the commit.
const tagCommits = new Map()
for (const line of lsRemote('--tags', REMOTE).split('\n')) {
    const [sha, ref] = line.split('\t')
    if (!ref) continue
    const name = ref.replace('refs/tags/', '')
    if (name.endsWith('^{}')) tagCommits.set(name.slice(0, -3), sha)
    else if (!tagCommits.has(name)) tagCommits.set(name, sha)
}
const mainSha = lsRemote(REMOTE, `refs/heads/${BRANCH}`).split('\t')[0]

const RC_RE = /^v(\d+)\.(\d+)\.0-rc\.(\d+)$/
const byVersion = (a, b) => {
    const [, a1, a2, a3] = a.match(RC_RE).map(Number)
    const [, b1, b2, b3] = b.match(RC_RE).map(Number)
    return a1 - b1 || a2 - b2 || a3 - b3
}
const rcTags = [...tagCommits.keys()].filter(t => RC_RE.test(t)).sort(byVersion)

// ── Which RC ───────────────────────────────────────────────────────────────────

let tag = (process.argv[2] || '').trim()
if (tag) {
    if (!RC_RE.test(tag)) fail(`RC must look like v0.19.0-rc.1, got '${tag}'`)
    if (!tagCommits.has(tag)) fail(`no tag ${tag} on GitHub. Cut one with make rc-next`)
} else {
    tag = rcTags.at(-1)
    if (!tag) {
        console.log('No release candidate on GitHub yet. Cut one with: make rc-next')
        process.exit(0)
    }
}
const sha = tagCommits.get(tag)
const final = tag.replace(/-rc\.\d+$/, '')
const newerRc = rcTags.filter(t => t.startsWith(`${final}-rc.`) && byVersion(t, tag) > 0).at(-1)

// ── GitHub API ─────────────────────────────────────────────────────────────────

// Errors are thrown, not process.exit()ed: on Windows, exiting while fetch
// sockets are open trips a libuv assertion. The bottom of the file reports them.
async function api(path) {
    const res = await fetch(API + path, {
        headers: {
            Accept: 'application/vnd.github+json',
            'X-GitHub-Api-Version': '2022-11-28',
            'User-Agent': 'lpdf-rc-check',
            ...(TOKEN ? { Authorization: `Bearer ${TOKEN}` } : {}),
        },
    })
    if (res.status === 404) return null
    if ((res.status === 403 || res.status === 429) && res.headers.get('x-ratelimit-remaining') === '0') {
        const reset = new Date(Number(res.headers.get('x-ratelimit-reset')) * 1000)
        throw new Error(`GitHub API rate limit reached (${TOKEN ? 'token' : '60 calls an hour without a token'}). It resets at ${reset.toLocaleTimeString()}. Set GH_TOKEN to raise it.`)
    }
    if (!res.ok) throw new Error(`GitHub API answered ${res.status} for ${path}`)
    return res.json()
}

// A tag push run carries the tag as head_branch; runs come newest first.
async function findCoreRun() {
    const runs = await api(`/repos/${OWNER}/${CORE}/actions/workflows/release.yml/runs?event=push&head_sha=${sha}&per_page=20`)
    return runs?.workflow_runs.find(r => r.head_branch === tag) ?? null
}

// rc.yml names each run "RC <tag>: …" (its run-name), which is how a run is
// matched to this RC. The colon keeps rc.1 from matching rc.10.
async function findRcRun(repo) {
    const runs = await api(`/repos/${OWNER}/${repo}/actions/workflows/rc.yml/runs?event=repository_dispatch&per_page=30`)
    if (runs === null) return { noWorkflow: true }
    return { run: runs.workflow_runs.find(r => (r.display_title || '').startsWith(`RC ${tag}:`)) ?? null }
}

// What a finished run has to say: our own ::error:: and ::notice:: lines, and
// the step that failed. GitHub's own annotations (the generic "Process
// completed with exit code 1.", runner and Node deprecation notices) are noise.
async function findMessages(repo, run) {
    if (run.status !== 'completed') return []
    const failed = run.conclusion !== 'success'
    const out = []
    const jobs = (await api(`/repos/${OWNER}/${repo}/actions/runs/${run.id}/jobs?per_page=20`))?.jobs ?? []
    for (const job of jobs) {
        if (job.conclusion === 'skipped') continue // e.g. core's final-release job on an RC run
        const step = job.steps?.find(s => s.conclusion === 'failure')
        if (step) out.push(`failed at: ${job.name} › ${step.name}`)
        // Success runs only matter for their notice, and core has none.
        if (!failed && repo === CORE) continue
        if (!failed && job.conclusion !== 'success') continue
        const annotations = (await api(`${new URL(job.check_run_url).pathname}/annotations`)) ?? []
        for (const a of annotations) {
            const text = a.message.split('\n')[0]
            if (a.annotation_level === 'failure' && !/^Process completed with exit code/.test(text)) out.push(`error: ${text}`)
            if (a.annotation_level === 'notice' && /unreleased commit/.test(text)) out.push(`notice: ${text}`)
        }
    }
    return out
}

// ── Collect ────────────────────────────────────────────────────────────────────

function stateOf(run) {
    if (run.status !== 'completed') return run.status === 'in_progress' ? 'running' : 'queued'
    if (run.conclusion === 'success') return 'ok'
    if (run.conclusion === 'cancelled') return 'cancelled'
    return 'FAILED'
}

async function collect() {
    const rows = []
    const core = await findCoreRun()
    const coreRow = { repo: CORE, file: 'release.yml', run: core }
    coreRow.state = core ? stateOf(core) : 'not started'
    coreRow.messages = core ? await findMessages(CORE, core) : []
    rows.push(coreRow)

    const coreDoneAt = core?.status === 'completed' ? Date.parse(core.updated_at) : null
    for (const repo of REPOS) {
        const row = { repo, file: 'rc.yml', messages: [] }
        const found = await findRcRun(repo)
        if (found.noWorkflow) {
            row.state = 'MISSING'
            row.messages.push(`no rc.yml on ${BRANCH}, so the dispatch started nothing`)
        } else if (found.run) {
            row.run = found.run
            row.state = stateOf(found.run)
            row.messages = await findMessages(repo, found.run)
        } else if (coreRow.state === 'ok' && Date.now() - coreDoneAt > MISSING_AFTER_MS) {
            row.state = 'MISSING'
            row.messages.push('core finished a while ago but no run started here')
        } else {
            row.state = coreRow.state === 'FAILED' || coreRow.state === 'cancelled' ? 'not started' : 'waiting'
        }
        rows.push(row)
    }
    return rows
}

// ── Show ───────────────────────────────────────────────────────────────────────

// Returns the exit code: 1 when something failed or is missing, else 0.
function show(rows) {
    const short = s => s.slice(0, 7)
    console.log('')
    console.log('-------------------------------')
    console.log(`>>> Release candidate ${tag}  (commit ${short(sha)}, for ${final})`)
    console.log('')
    if (newerRc) console.log(`  Note: ${newerRc} is newer. Check that one: make rc-check RC=${newerRc}\n`)
    for (const r of rows) {
        const link = r.run ? r.run.html_url : ''
        console.log(`  ${r.repo.padEnd(12)} ${r.file.padEnd(12)} ${r.state.padEnd(12)} ${link}`)
        for (const m of r.messages) console.log(`  ${''.padEnd(26)} ${m}`)
    }
    console.log('')

    const bad = rows.filter(r => ['FAILED', 'cancelled', 'MISSING'].includes(r.state))
    const busy = rows.filter(r => ['running', 'queued', 'waiting', 'not started'].includes(r.state))
    const vscode = rows.find(r => r.repo === 'lpdf-vscode')

    if (bad.length) {
        console.log(`Not releasable: ${bad.map(r => `${r.repo} ${r.state.toLowerCase()}`).join(', ')}.`)
        console.log('A code problem: fix it on main, then make rc-next for the next RC.')
        console.log('A flaky run: open it and use "Re-run failed jobs", then check again.')
        return 1
    }
    if (busy.length) {
        const done = rows.filter(r => r.state === 'ok').length
        console.log(`Still going: ${done} of ${rows.length} runs passed so far. Check again in a few minutes.`)
        return 0
    }

    console.log(`All ${rows.length} runs passed.`)
    if (tagCommits.has(final)) {
        console.log(`${final} is already released.`)
        return 0
    }
    console.log('')
    console.log('Before releasing:')
    console.log(`  1. The extension has no automated tests. Download lpdf-${tag} (the .vsix)`)
    console.log(`     from ${vscode.run.html_url}`)
    console.log('     then: code --install-extension lpdf.vsix, open an example, check preview and export.')
    console.log(`  2. Publish ${final} on this RC's commit, with typed release notes. This opens`)
    console.log('     the release form with tag and commit filled in:')
    console.log(`     ${WEB}/${OWNER}/${CORE}/releases/new?tag=${final}&target=${sha}&title=${final}`)
    if (mainSha && mainSha !== sha) {
        console.log('')
        console.log(`  Note: GitHub's ${BRANCH} has moved on since this RC. The release still goes`)
        console.log(`        on ${short(sha)}; newer commits wait for the next RC.`)
    }
    return 0
}

// exitCode, not process.exit(): let Node close the fetch sockets itself.
try {
    process.exitCode = show(await collect())
} catch (error) {
    console.error(`rc-check: ${error.message}`)
    process.exitCode = 1
}
