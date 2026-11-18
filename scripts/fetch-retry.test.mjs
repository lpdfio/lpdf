import assert from 'node:assert/strict';
import test from 'node:test';

import { fetchBytes, MAX_RETRY_AFTER } from './fetch-retry.mjs';

const FILE = 'https://repo.example/file.jar';

/** An answer with `status`, as `fetch` gives it. */
const answer = (status, statusText, body = '', headers = {}) => new Response(status === 200 ? body : null, { status, statusText, headers });

/** A `fetch` that gives the answers in turn, and throws the ones that are errors, and a count of its calls. */
function fetching(...answers) {
    const fetchFn = async () => {
        fetchFn.calls++;
        const next = answers.shift();
        if (next instanceof Error) throw next;
        return next;
    };
    fetchFn.calls = 0;
    return fetchFn;
}

/** A `sleep` that waits for nothing, and notes how many seconds it was asked to. */
function sleeping() {
    const sleep = async (seconds) => { sleep.waited.push(seconds); };
    sleep.waited = [];
    return sleep;
}

const WAITS = [1, 2, 3];

test('a download that gets a 429 twice and then the file returns the file after two waits', async () => {
    const fetchFn = fetching(answer(429, 'Too Many Requests'), answer(429, 'Too Many Requests'), answer(200, 'OK', 'the jar'));
    const sleep = sleeping();
    const bytes = await fetchBytes(FILE, { fetchFn, sleep, waits: WAITS });
    assert.equal(bytes.toString(), 'the jar');
    assert.equal(fetchFn.calls, 3);
    assert.deepEqual(sleep.waited, [1, 2]);
});

test('a server error is tried again like a 429', async () => {
    const fetchFn = fetching(answer(503, 'Service Unavailable'), answer(200, 'OK', 'the jar'));
    const sleep = sleeping();
    assert.equal((await fetchBytes(FILE, { fetchFn, sleep, waits: WAITS })).toString(), 'the jar');
    assert.deepEqual(sleep.waited, [1]);
});

test('a Retry-After is the wait when the answer gives one, up to the limit', async () => {
    const fetchFn = fetching(
        answer(429, 'Too Many Requests', '', { 'Retry-After': '7' }),
        answer(429, 'Too Many Requests', '', { 'Retry-After': '99999' }),
        answer(429, 'Too Many Requests', '', { 'Retry-After': 'Wed, 21 Oct 2026 07:28:00 GMT' }),
        answer(200, 'OK', 'the jar'),
    );
    const sleep = sleeping();
    await fetchBytes(FILE, { fetchFn, sleep, waits: WAITS });
    // A date is not a number of seconds, and is read as no Retry-After: the third wait of the list.
    assert.deepEqual(sleep.waited, [7, MAX_RETRY_AFTER, 3]);
});

test('a 404 fails at once with its status and no wait', async () => {
    const fetchFn = fetching(answer(404, 'Not Found'));
    const sleep = sleeping();
    await assert.rejects(fetchBytes(FILE, { fetchFn, sleep, waits: WAITS }), { message: `could not download ${FILE}: 404 Not Found` });
    assert.equal(fetchFn.calls, 1);
    assert.deepEqual(sleep.waited, []);
});

test('every try used fails with the status of the last', async () => {
    const fetchFn = fetching(answer(503, 'Service Unavailable'), answer(503, 'Service Unavailable'), answer(429, 'Too Many Requests'), answer(429, 'Too Many Requests'));
    const sleep = sleeping();
    await assert.rejects(fetchBytes(FILE, { fetchFn, sleep, waits: WAITS }), { message: `could not download ${FILE}: 429 Too Many Requests` });
    assert.equal(fetchFn.calls, WAITS.length + 1);
    assert.deepEqual(sleep.waited, WAITS);
});

test('a network error is tried again', async () => {
    const fetchFn = fetching(new TypeError('fetch failed'), answer(200, 'OK', 'the jar'));
    const sleep = sleeping();
    assert.equal((await fetchBytes(FILE, { fetchFn, sleep, waits: WAITS })).toString(), 'the jar');
    assert.deepEqual(sleep.waited, [1]);
});

test('every try used fails with the error of the last when none gets an answer', async () => {
    const fetchFn = fetching(new TypeError('fetch failed'), new TypeError('fetch failed'), new TypeError('fetch failed'), new TypeError('socket hang up'));
    await assert.rejects(fetchBytes(FILE, { fetchFn, sleep: sleeping(), waits: WAITS }), { message: `could not download ${FILE}: socket hang up` });
});
