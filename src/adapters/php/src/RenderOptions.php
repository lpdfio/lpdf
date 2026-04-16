<?php

declare(strict_types=1);

namespace Lpdf;

final class RenderOptions
{
    /**
     * @param array<string, string>|null $fontBytes Font name → raw TTF/OTF bytes.
     */
    public function __construct(
        public readonly ?string $wasmBinary = null,   // path to .wasm file
        public readonly ?string $wasmRunner = null,   // runner executable name/path
        public readonly ?string $createdOn  = null,   // ISO 8601 for PDF /CreationDate
        public readonly ?array  $fontBytes  = null,   // font name → raw bytes
    ) {}
}
