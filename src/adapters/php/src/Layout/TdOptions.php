<?php

declare(strict_types=1);

namespace Lpdf\Layout;

final readonly class TdOptions
{
    public function __construct(
        public ?string $padding    = null,
        public ?string $background = null,
        public ?string $align      = null,
        public ?string $valign     = null,
        public ?string $border     = null,
        public ?string $radius     = null,
        public ?string $gap        = null,
        public ?string $debug      = null,
    ) {}
}
