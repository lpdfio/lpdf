<?php

declare(strict_types=1);

namespace Lpdf\Layout;

final readonly class TableOptions
{
    public function __construct(
        public string  $cols,
        public ?string $border     = null,
        public ?string $stripe     = null,
        public ?string $gap        = null,
        public ?string $padding    = null,
        public ?string $background = null,
        public ?string $width      = null,
        public ?string $height     = null,
        public ?string $debug      = null,
    ) {}
}
