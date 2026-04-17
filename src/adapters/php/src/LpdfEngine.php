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
     * @throws LpdfRenderException On render or process error.
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

        // Auto-load fonts declared via src= that haven't been explicitly provided.
        $fontSrcs = $method === 'render_pdf'
            ? self::xmlFontSrcs($inputStr)
            : self::jsonFontSrcs($inputStr);
        foreach ($fontSrcs as $name => $path) {
            if (!array_key_exists($name, $mergedFonts) && is_readable($path)) {
                $bytes = file_get_contents($path);
                if ($bytes !== false) {
                    $mergedFonts[$name] = $bytes;
                }
            }
        }

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
            throw new LpdfRenderException('Unexpected response from WASI process.');
        }

        $bytes = base64_decode($response['pdf'], strict: true);
        if ($bytes === false) {
            throw new LpdfRenderException('Failed to decode base64 PDF response.');
        }

        return $bytes;
    }

    /** @return array<string,string> Font name → file path from `<font name="…" src="…">` tags. */
    private static function xmlFontSrcs(string $xml): array
    {
        $srcs = [];
        preg_match_all('/<font\s[^>]*>/', $xml, $m);
        foreach ($m[0] as $tag) {
            if (preg_match('/\bname="([^"]*)"/', $tag, $nm) &&
                preg_match('/\bsrc="([^"]*)"/', $tag, $src)) {
                $srcs[$nm[1]] = $src[1];
            }
        }
        return $srcs;
    }

    /** @return array<string,string> Font name → file path from a serialised tree's `tokens.fonts`. */
    private static function jsonFontSrcs(string $json): array
    {
        $srcs = [];
        $doc  = json_decode($json, true, 512, JSON_THROW_ON_ERROR);
        foreach ($doc['attrs']['tokens']['fonts'] ?? [] as $name => $def) {
            if (isset($def['src'])) {
                $srcs[$name] = $def['src'];
            }
        }
        return $srcs;
    }

    private static function defaultBinary(): string
    {
        return \dirname(__DIR__) . '/resources/lpdf-wasi.wasm';
    }
}
