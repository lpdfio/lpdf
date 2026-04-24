<?php

declare(strict_types=1);

namespace Lpdf\Layout;

final readonly class ImgOptions
{
    public function __construct(
        public string  $name,
        public ?string $height     = null,
        public ?string $width      = null,
        public ?string $gap        = null,
        public ?string $padding    = null,
        public ?string $background = null,
        public ?string $border     = null,
        public ?string $radius     = null,
        public ?string $debug      = null,
    ) {}
}
