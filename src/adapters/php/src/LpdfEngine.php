<?php

declare(strict_types=1);

namespace Lpdf;

final class LpdfEngine
{
    /** @var array<string, string> Font name → raw TTF/OTF bytes */
    private array $fonts = [];

    public function __construct(
        private readonly string        $licenseKey,
        private readonly RenderOptions $options = new RenderOptions(),
    ) {}

    public function loadFont(string $name, string $bytes): static
    {
        $this->fonts[$name] = $bytes;
        return $this;
    }

    /**
     * Render an lpdf XML document and return raw PDF bytes.
     *
     * @throws \RuntimeException on render error.
     */
    public function renderPdf(string $xml): string
    {
        $runner = new WasmRunner(
            wasmBinary: $this->options->wasmBinary ?? self::defaultBinary(),
            wasmRunner: $this->options->wasmRunner ?? 'wasmtime',
        );

        $payload = [
            'method' => 'render_pdf',
            'key'    => $this->licenseKey,
            'input'  => $xml,
        ];

        if ($this->fonts !== []) {
            $payload['fonts'] = array_map('base64_encode', $this->fonts);
        }

        if ($this->options->createdOn !== null) {
            $payload['created_on'] = $this->options->createdOn;
        }

        $response = $runner->invoke($payload);

        if (!isset($response['pdf'])) {
            throw new \RuntimeException('Unexpected response from WASI process.');
        }

        $bytes = base64_decode($response['pdf'], strict: true);
        if ($bytes === false) {
            throw new \RuntimeException('Failed to decode base64 PDF response.');
        }

        return $bytes;
    }

    private static function defaultBinary(): string
    {
        return \dirname(__DIR__) . '/resources/lpdf-wasi.wasm';
    }
}
