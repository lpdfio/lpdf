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

        // Extract glyph advance widths from font bytes and pass to the WASI engine
        // so the Rust layout pass measures custom-font text accurately.
        $metrics = [];
        foreach ($mergedFonts as $name => $fontBytes) {
            $w = self::extractFontWidths($fontBytes);
            if ($w !== null) {
                $metrics[$name] = $w;
            }
        }
        if ($metrics !== []) {
            $payload['metrics'] = $metrics;
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

    /**
     * Parse the head/hhea/cmap/hmtx tables from a TrueType or OpenType font binary
     * and return per-glyph advance widths for printable ASCII (code points 32–126),
     * normalised to 1/1000 em units — the same format the Rust layout engine expects.
     *
     * Returns null if the font cannot be parsed (WOFF/WOFF2/unsupported cmap format).
     *
     * @return array{default: int, ascii: list<int>}|null
     */
    public static function extractFontWidths(string $bytes): ?array
    {
        $len = strlen($bytes);
        if ($len < 12) {
            return null;
        }

        // Helper: read big-endian u16 / u32 / i16 from $bytes at offset $off
        $u16 = static function (int $off) use ($bytes): int {
            return (ord($bytes[$off]) << 8) | ord($bytes[$off + 1]);
        };
        $u32 = static function (int $off) use ($bytes): int {
            return ((ord($bytes[$off]) << 24) | (ord($bytes[$off + 1]) << 16)
                  | (ord($bytes[$off + 2]) << 8) |  ord($bytes[$off + 3])) & 0xFFFFFFFF;
        };
        $i16 = static function (int $off) use ($bytes): int {
            $v = (ord($bytes[$off]) << 8) | ord($bytes[$off + 1]);
            return $v >= 0x8000 ? $v - 0x10000 : $v;
        };

        // ── sfnt table directory ─────────────────────────────────────────────
        $numTables = $u16(4);
        $tables    = [];
        for ($i = 0; $i < $numTables; $i++) {
            $b   = 12 + $i * 16;
            if ($b + 16 > $len) {
                return null;
            }
            $tag        = substr($bytes, $b, 4);
            $tables[$tag] = $u32($b + 8);
        }

        if (!isset($tables['head'], $tables['cmap'], $tables['hmtx'], $tables['hhea'])) {
            return null;
        }

        // ── units-per-em (head table, offset 18) ─────────────────────────────
        $upm = $u16($tables['head'] + 18);
        if ($upm === 0) {
            return null;
        }

        // ── numOfLongHorMetrics (hhea, offset 34) ─────────────────────────────
        $numHMetrics = $u16($tables['hhea'] + 34);
        if ($numHMetrics === 0) {
            return null;
        }

        // ── glyph advance width from hmtx ────────────────────────────────────
        $hmtxBase   = $tables['hmtx'];
        $getAdvance = static function (int $glyphId) use ($bytes, $hmtxBase, $numHMetrics, $u16): int {
            $idx = min($glyphId, $numHMetrics - 1);
            return $u16($hmtxBase + $idx * 4);
        };

        // ── find Unicode BMP cmap subtable ────────────────────────────────────
        $cmapBase   = $tables['cmap'];
        $numEncTbls = $u16($cmapBase + 2);
        $subtableOff  = -1;
        $bestPriority = 999;
        for ($i = 0; $i < $numEncTbls; $i++) {
            $b          = $cmapBase + 4 + $i * 8;
            $platformId = $u16($b);
            $encodingId = $u16($b + 2);
            $off        = $cmapBase + $u32($b + 4);
            if ($platformId === 3 && $encodingId === 1 && $bestPriority > 0) {
                $subtableOff  = $off;
                $bestPriority = 0;
            } elseif ($platformId === 0 && $bestPriority > 1) {
                $subtableOff  = $off;
                $bestPriority = 1;
            }
        }
        if ($subtableOff < 0) {
            return null;
        }

        // ── parse cmap format 4 ───────────────────────────────────────────────
        if ($u16($subtableOff) !== 4) {
            return null;
        }

        $segCount      = $u16($subtableOff + 6) >> 1;
        $endCodesOff   = $subtableOff + 14;
        $startCodesOff = $endCodesOff   + $segCount * 2 + 2; // +2 for reservedPad
        $idDeltaOff    = $startCodesOff + $segCount * 2;
        $idRangeOff    = $idDeltaOff    + $segCount * 2;

        $getGlyphId = static function (int $cp) use (
            $bytes, $segCount, $endCodesOff, $startCodesOff,
            $idDeltaOff, $idRangeOff, $u16, $i16
        ): int {
            for ($s = 0; $s < $segCount; $s++) {
                $end = $u16($endCodesOff + $s * 2);
                if ($cp > $end) {
                    continue;
                }
                $start = $u16($startCodesOff + $s * 2);
                if ($cp < $start) {
                    return 0;
                }
                $delta    = $i16($idDeltaOff + $s * 2);
                $rangeOff = $u16($idRangeOff + $s * 2);
                if ($rangeOff === 0) {
                    return ($cp + $delta) & 0xffff;
                }
                $glyphOff = $idRangeOff + $s * 2 + $rangeOff + ($cp - $start) * 2;
                $glyphId  = $u16($glyphOff);
                return $glyphId === 0 ? 0 : ($glyphId + $delta) & 0xffff;
            }
            return 0;
        };

        // ── sample ASCII range (32–126) ───────────────────────────────────────
        $ascii = [];
        $sum   = 0;
        $count = 0;
        for ($cp = 32; $cp <= 126; $cp++) {
            $glyphId = $getGlyphId($cp);
            $adv     = $glyphId > 0 ? $getAdvance($glyphId) : $getAdvance(0);
            $w       = (int) round($adv * 1000 / $upm);
            $ascii[] = $w;
            if ($w > 0) {
                $sum += $w;
                $count++;
            }
        }

        $default = $count > 0 ? (int) round($sum / $count) : 500;

        return ['default' => $default, 'ascii' => $ascii];
    }
}
