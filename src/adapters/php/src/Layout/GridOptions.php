<?php

declare(strict_types=1);

namespace Lpdf\Layout;

final readonly class GridOptions
{
    public function __construct(
        public ?string $cols       = null,
        public ?string $colWidth   = null,
        public ?string $gap        = null,
        public ?string $equal      = null,
        public ?string $padding    = null,
        public ?string $background = null,
        public ?string $width      = null,
        public ?string $height     = null,
        public ?string $border     = null,
        public ?string $radius     = null,
        public ?string $debug      = null,
    ) {}
}
