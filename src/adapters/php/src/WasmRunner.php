<?php

declare(strict_types=1);

namespace Lpdf;

final class WasmRunner
{
    public function __construct(
        private readonly string $wasmBinary,
        private readonly string $wasmRunner = 'wasmtime',
    ) {}

    /**
     * @param  array<string, mixed> $payload  Already-built request array.
     * @return array<string, mixed>            Decoded response.
     * @throws LpdfRenderException On process or render error.
     */
    public function invoke(array $payload): array
    {
        $cmd = escapeshellcmd($this->wasmRunner)
             . ' run '
             . escapeshellarg($this->wasmBinary);

        $proc = proc_open($cmd, [
            0 => ['pipe', 'r'],   // stdin
            1 => ['pipe', 'w'],   // stdout
            2 => ['pipe', 'w'],   // stderr
        ], $pipes);

        if ($proc === false) {
            throw new LpdfRenderException('Failed to start WASI process.');
        }

        fwrite($pipes[0], json_encode($payload, JSON_THROW_ON_ERROR));
        fclose($pipes[0]);

        $out = stream_get_contents($pipes[1]);
        $err = stream_get_contents($pipes[2]);
        fclose($pipes[1]);
        fclose($pipes[2]);
        proc_close($proc);

        if ($out === false || $out === '') {
            throw new LpdfRenderException("WASI process produced no output. Stderr: $err");
        }

        $response = json_decode($out, true, 512, JSON_THROW_ON_ERROR);

        if (isset($response['error'])) {
            throw new LpdfRenderException("lpdf render error: {$response['error']}");
        }

        return $response;
    }
}
