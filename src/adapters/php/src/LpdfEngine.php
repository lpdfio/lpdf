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
     * @param  RenderOptions|null $callOptions Per-call overrides merged with constructor options.
     * @throws \RuntimeException on render error.
     */
    public function renderPdf(string $xml, ?RenderOptions $callOptions = null): string
    {
        $runner = new WasmRunner(
            wasmBinary: $callOptions?->wasmBinary ?? $this->options->wasmBinary ?? self::defaultBinary(),
            wasmRunner: $callOptions?->wasmRunner ?? $this->options->wasmRunner ?? 'wasmtime',
        );

        // Merge fonts: loadFont() calls take precedence, then per-call fontBytes,
        // then constructor-level fontBytes. Per-call wins over constructor on collision.
        $mergedFonts = array_merge(
            $this->options->fontBytes ?? [],
            $callOptions?->fontBytes  ?? [],
            $this->fonts,
        );

        $payload = [
            'method' => 'render_pdf',
            'key'    => $this->licenseKey,
            'input'  => $xml,
        ];

        if ($mergedFonts !== []) {
            $payload['fonts'] = array_map('base64_encode', $mergedFonts);
        }

        $createdOn = $callOptions?->createdOn ?? $this->options->createdOn;
        if ($createdOn !== null) {
            $payload['created_on'] = $createdOn;
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
