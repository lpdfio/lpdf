<?php

declare(strict_types=1);

namespace Lpdf;

final class LpdfEngine
{
    /** @var array<string, string> Font name → raw TTF/OTF bytes */
    private array $fonts = [];

    /** @var array<string, string> Image name → raw image bytes (PNG/JPEG/WebP/…) */
    private array $images = [];

    /**
     * Optional RC4-128 encryption config.
     * Keys: user_password (string), owner_password (string), permissions (array).
     * @var array{user_password: string, owner_password: string, permissions: array<string, bool>}|null
     */
    private ?array $encrypt = null;

    public function __construct(
        private readonly string        $licenseKey,
        private readonly RenderOptions $options = new RenderOptions(),
    ) {}

    /**
     * Configure RC4-128 encryption for all subsequent renderPdf() calls.
     * Pass null to clear previously set encryption.
     *
     * @param string                    $userPassword  Open password (empty = no open password).
     * @param string                    $ownerPassword Owner (permissions) password.
     * @param array<string, bool>       $permissions   Flags: print, modify, copy, annotate,
     *                                                 fill_forms, accessibility, assemble, print_hq.
     *                                                 Omitted flags default to true (allowed).
     */
    public function setEncryption(string $userPassword, string $ownerPassword, array $permissions = []): static
    {
        $this->encrypt = [
            'user_password'  => $userPassword,
            'owner_password' => $ownerPassword,
            'permissions'    => $permissions,
        ];
        return $this;
    }

    /**
     * Remove any previously configured encryption.
     */
    public function clearEncryption(): static
    {
        $this->encrypt = null;
        return $this;
    }

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
        foreach ($fontSrcs as $key => $path) {
            if (!array_key_exists($key, $mergedFonts) && is_readable($path)) {
                $bytes = file_get_contents($path);
                if ($bytes !== false) {
                    $mergedFonts[$key] = $bytes;
                }
            }
        }

        // Merge images: same precedence order as fonts.
        $mergedImages = array_merge(
            $this->options->imageBytes ?? [],
            $callOptions?->imageBytes  ?? [],
            $this->images,
        );

        // Auto-load images declared via src= that haven't been explicitly provided.
        $imageSrcs = $method === 'render_pdf'
            ? self::xmlImageSrcs($inputStr)
            : self::jsonImageSrcs($inputStr);
        foreach ($imageSrcs as $key => $path) {
            if (!array_key_exists($key, $mergedImages) && is_readable($path)) {
                $bytes = file_get_contents($path);
                if ($bytes !== false) {
                    $mergedImages[$key] = $bytes;
                }
            }
        }

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

        if ($this->encrypt !== null) {
            $payload['encrypt'] = $this->encrypt;
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

    /**
     * Convert an LpdfDocument tree (built with LpdfKit) to an lpdf XML string.
     *
     * The conversion is performed by the Rust core running as a WASI subprocess,
     * so the output is identical to the XML produced by the other adapters.
     *
     * @param  LpdfDocument       $doc         The document tree to serialise.
     * @param  string|null        $wasmBinary  Path to lpdf-wasi.wasm (defaults to bundled binary).
     * @param  string             $wasmRunner  WASI runtime executable (default: wasmtime).
     * @throws LpdfRenderException On process or serialisation error.
     */
    public static function kitToXml(
        LpdfDocument $doc,
        ?string $wasmBinary = null,
        string $wasmRunner = 'wasmtime',
    ): string {
        $runner = new WasmRunner(
            wasmBinary: $wasmBinary ?? self::defaultBinary(),
            wasmRunner: $wasmRunner,
        );
        $payload  = [
            'method' => 'kit_to_xml',
            'input'  => json_encode($doc, JSON_THROW_ON_ERROR | JSON_UNESCAPED_UNICODE),
        ];
        $response = $runner->invoke($payload);
        if (!isset($response['xml'])) {
            throw new LpdfRenderException('Unexpected response from WASI process (kit_to_xml).');
        }
        return $response['xml'];
    }

    /** @return array<string,string> Font ref??name → file path from `<font name="…" src="…">` tags. */
    private static function xmlFontSrcs(string $xml): array
    {
        $srcs = [];
        preg_match_all('/<font\s[^>]*>/', $xml, $m);
        foreach ($m[0] as $tag) {
            if (preg_match('/\bname="([^"]*)"/', $tag, $nm) &&
                preg_match('/\bsrc="([^"]*)"/', $tag, $src)) {
                $key = preg_match('/\bref="([^"]*)"/', $tag, $ref) ? $ref[1] : $nm[1];
                $srcs[$key] = $src[1];
            }
        }
        return $srcs;
    }

    /** @return array<string,string> Image ref??name → file path from `<image name="…" src="…">` tags. */
    private static function xmlImageSrcs(string $xml): array
    {
        $srcs = [];
        preg_match_all('/<image\s[^>]*>/', $xml, $m);
        foreach ($m[0] as $tag) {
            if (preg_match('/\bname="([^"]*)"/', $tag, $nm) &&
                preg_match('/\bsrc="([^"]*)"/', $tag, $src)) {
                $key = preg_match('/\bref="([^"]*)"/', $tag, $ref) ? $ref[1] : $nm[1];
                $srcs[$key] = $src[1];
            }
        }
        return $srcs;
    }

    /** @return array<string,string> Font ref??name → file path from a serialised tree's `tokens.fonts`. */
    private static function jsonFontSrcs(string $json): array
    {
        $srcs = [];
        $doc  = json_decode($json, true, 512, JSON_THROW_ON_ERROR);
        foreach ($doc['attrs']['tokens']['fonts'] ?? [] as $name => $def) {
            if (isset($def['src'])) {
                $key = $def['ref'] ?? $name;
                $srcs[$key] = $def['src'];
            }
        }
        return $srcs;
    }

    /** @return array<string,string> Image ref??name → file path from a serialised tree's `tokens.images`. */
    private static function jsonImageSrcs(string $json): array
    {
        $srcs = [];
        $doc  = json_decode($json, true, 512, JSON_THROW_ON_ERROR);
        foreach ($doc['attrs']['tokens']['images'] ?? [] as $name => $def) {
            if (isset($def['src'])) {
                $key = $def['ref'] ?? $name;
                $srcs[$key] = $def['src'];
            }
        }
        return $srcs;
    }

    private static function defaultBinary(): string
    {
        return \dirname(__DIR__) . '/resources/lpdf-wasi.wasm';
    }
}
