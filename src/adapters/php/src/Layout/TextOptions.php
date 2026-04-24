<?php

declare(strict_types=1);

namespace Lpdf\Layout;

final readonly class TextOptions
{
    public function __construct(
        public ?string $font       = null,
        public ?string $fontSize   = null,
        public ?string $textAlign  = null,
        public ?string $color      = null,
        public ?string $bold       = null,
        public ?string $end        = null,
        public ?string $width      = null,
        public ?string $height     = null,
        public ?string $padding    = null,
        public ?string $background = null,
        public ?string $border     = null,
        public ?string $radius     = null,
    ) {}
}
