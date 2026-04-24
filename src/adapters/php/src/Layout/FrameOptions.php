<?php

declare(strict_types=1);

namespace Lpdf\Layout;

final readonly class FrameOptions
{
    public function __construct(
        public ?string $width      = null,
        public ?string $height     = null,
        public ?string $padding    = null,
        public ?string $background = null,
        public ?string $border     = null,
        public ?string $radius     = null,
        public ?string $align      = null,
        public ?string $debug      = null,
    ) {}
}
