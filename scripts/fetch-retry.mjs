// Downloads for the scripts that fetch a file in CI, such as the Mustang CLI of the Factur-X check
// (`scripts/check-facturx.mjs`). A server such as Maven Central answers "too many requests" to the addresses CI's
// runners share, and a download that fails there fails the run, though nothing in the repo is wrong. So a fetch is
// tried again after a wait when the answer is a 429, a server error (5xx) or none at all, and given up at once on any
// other status, since another try would be told the same.

/** The seconds to wait before each try after the first: a file is fetched up to `RETRY_WAITS.length + 1` times. */
export const RETRY_WAITS = [10, 30, 60];

/** The most seconds a `Retry-After` of the server is waited, so a server can't hold a run for an hour. */
export const MAX_RETRY_AFTER = 120;

const sleepFor = (seconds) => new Promise((resolve) => setTimeout(resolve, seconds * 1000));

/** Whether another try may be told something else: too many requests, or a server that failed. */
const worthTrying = (status) => status === 429 || status >= 500;

/** The seconds a `Retry-After` asks for, up to the limit, or `undefined` when it gives none or gives a date. */
function retryAfter(response) {
    const value = response.headers.get('retry-after')?.trim();
    return value !== undefined && /^\d+$/.test(value) ? Math.min(Number(value), MAX_RETRY_AFTER) : undefined;
}

/** What a fetch that failed says: its message and, from its `cause`, why (ECONNREFUSED, ENOTFOUND, ETIMEDOUT...). */
function describe(error) {
    const reason = error.cause?.code ?? error.cause?.message;
    return reason ? `${error.message} (${reason})` : error.message;
}

/**
 * One try at `url`: its bytes, or why there are none, whether another try may get them, and the seconds the server
 * asked to wait. A body that breaks off is a network error, as no answer at all is.
 */
async function tryOnce(url, fetchFn) {
    try {
        const response = await fetchFn(url);
        if (response.ok) return { bytes: Buffer.from(await response.arrayBuffer()) };
        return { failure: `${response.status} ${response.statusText}`, again: worthTrying(response.status), wait: retryAfter(response) };
    } catch (error) {
        return { failure: describe(error), again: true };
    }
}

/**
 * The bytes at `url`. `fetchFn` and `sleep` are `fetch` and a wait in seconds, and `waits` the seconds to wait before
 * each try after the first: they are given by the tests, which run no server and wait for nothing. Throws, when the
 * file can't be had, `could not download <url>: <status and its text>`, or after the url the message of the last error
 * and its cause, such as `fetch failed (ECONNREFUSED)`.
 */
export async function fetchBytes(url, { fetchFn = fetch, sleep = sleepFor, waits = RETRY_WAITS } = {}) {
    for (let attempt = 0; ; attempt++) {
        const outcome = await tryOnce(url, fetchFn);
        if (outcome.bytes) return outcome.bytes;
        if (!outcome.again || attempt === waits.length) throw new Error(`could not download ${url}: ${outcome.failure}`);
        await sleep(outcome.wait ?? waits[attempt]);
    }
}
