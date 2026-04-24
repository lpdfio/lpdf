<?php

declare(strict_types=1);

namespace Lpdf\Layout;

final readonly class BarcodeOptions
{
    public function __construct(
        public string  $type,
        public string  $data,
        public ?string $size       = null,
        public ?string $width      = null,
        public ?string $height     = null,
        public ?string $ec         = null,
        public ?string $hrt        = null,
        public ?string $color      = null,
        public ?string $background = null,
        public ?string $debug      = null,
    ) {}
}
