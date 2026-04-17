<?php

declare(strict_types=1);

namespace Lpdf;

final class LpdfEngine
{
    /** @var array<string, string> Font name → raw TTF/OTF bytes */
    private array $fonts = [];

    /** @var array<string, string> Image name → raw image bytes (PNG/JPEG/WebP/…) */
    private array $images = [];

    public function __construct(
        private readonly string        $licenseKey,
        private readonly RenderOptions $options = new RenderOptions(),
    ) {}

    /**
     * Register a custom font asset.
     *
     * The name must match the `ref` attribute of a `<font>` element in the
     * XML `<assets>` block, e.g.:
     *   XML:  <font name="heading" ref="inter-bold"/>
     *   PHP:  $engine->loadFont('inter-bold', file_get_contents('InterBold.ttf'));
     */
    public function loadFont(string $name, string $bytes): static
    {
        $this->fonts[$name] = $bytes;
        return $this;
    }

    /**
     * Register an image asset.
     *
     * The name must match the `name` attribute of an `<image>` element in the
     * XML `<assets>` block, e.g.:
     *   XML:  <image name="logo"/>
     *   PHP:  $engine->loadImage('logo', file_get_contents('logo.png'));
     */
    public function loadImage(string $name, string $bytes): static
    {
        $this->images[$name] = $bytes;
        return $this;
    }

    /**
     * Render an lpdf XML string or LpdfDocument tree and return raw PDF bytes.
     *
     * @param  string|LpdfDocument $input       XML string or a tree built with LpdfKit.
     * @param  RenderOptions|null  $callOptions Per-call overrides merged with constructor options.
     * @throws \RuntimeException on render error.
     */
    public function renderPdf(string|LpdfDocument $input, ?RenderOptions $callOptions = null): string
    {
        if ($input instanceof LpdfDocument) {
            $method   = 'render_tree_pdf';
            $inputStr = json_encode($input, JSON_THROW_ON_ERROR | JSON_UNESCAPED_UNICODE);
        } else {
            $method   = 'render_pdf';
            $inputStr = $input;
        }

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

        // Merge images: same precedence order as fonts.
        $mergedImages = array_merge(
            $this->options->imageBytes ?? [],
            $callOptions?->imageBytes  ?? [],
            $this->images,
        );

        $payload = [
            'method' => $method,
            'key'    => $this->licenseKey,
            'input'  => $inputStr,
        ];

        if ($mergedFonts !== []) {
            $payload['fonts'] = array_map('base64_encode', $mergedFonts);
        }

        if ($mergedImages !== []) {
            $payload['images'] = array_map('base64_encode', $mergedImages);
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
