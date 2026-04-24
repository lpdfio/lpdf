<?php

declare(strict_types=1);

namespace Lpdf\Engine;

final readonly class EngineOptions
{
    /**
     * @param array<string, string>|null $fontBytes  Font name → raw TTF/OTF bytes.
     * @param array<string, string>|null $imageBytes Image name → raw image bytes (PNG/JPEG/WebP/…).
     */
    public function __construct(
        public ?string $wasmBinary  = null,   // path to .wasm file
        public ?string $wasmRunner  = null,   // runner executable name/path
        public ?string $createdOn   = null,   // ISO 8601 for PDF /CreationDate
        public ?array  $fontBytes   = null,   // font name → raw bytes
        public ?array  $imageBytes  = null,   // image name → raw bytes
        public ?int    $timeout     = null,   // WASI process timeout in seconds (default: 30)
    ) {}
}
