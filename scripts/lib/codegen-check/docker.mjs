// Runs the program of a language in a Docker image, for a machine that doesn't have the language, such as the Mac for PHP
// and Python (`make check-codegen-php`). The image is the SDK's own, which has the SDK in /app.
//
// The program and the files it reads are at the same path in the container as on the machine: the scratch folder, and the
// folder of the test fixtures and assets, read-only. `--mount <folder>:<path>` adds the rest, such as the engine the
// image's copy of the SDK is to run.

import path from 'node:path';

import { run } from './common.mjs';

/** Where the SDK is in the image. */
export const SDK_IN_DOCKER = '/app';

/**
 * Runs `command` with `args`: here when `docker` is undefined, else in a container of `docker.image` that has
 * `docker.mounts` ('<folder>:<path>') besides the scratch folder `work` and the folder that holds `base`.
 */
export function inDocker(docker, command, args, { work, base }) {
    if (!docker) return run(command, args);
    const mounts = [`${work}:${work}`, `${path.dirname(base)}:${path.dirname(base)}:ro`, ...docker.mounts];
    return run('docker', ['run', '--rm', ...mounts.flatMap((mount) => ['-v', mount]), '-w', SDK_IN_DOCKER, docker.image, command, ...args]);
}
