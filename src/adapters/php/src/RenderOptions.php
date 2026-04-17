<?php

declare(strict_types=1);

namespace Lpdf;

final class RenderOptions
{
    /**
     * @param array<string, string>|null $fontBytes  Font name → raw TTF/OTF bytes.
     * @param array<string, string>|null $imageBytes Image name → raw image bytes (PNG/JPEG/WebP/…).
     */
    public function __construct(
        public readonly ?string $wasmBinary  = null,   // path to .wasm file
        public readonly ?string $wasmRunner  = null,   // runner executable name/path
        public readonly ?string $createdOn   = null,   // ISO 8601 for PDF /CreationDate
        public readonly ?array  $fontBytes   = null,   // font name → raw bytes
        public readonly ?array  $imageBytes  = null,   // image name → raw bytes
    ) {}
}
