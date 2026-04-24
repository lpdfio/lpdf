<?php

declare(strict_types=1);

namespace Lpdf\Layout;

final readonly class SpanOptions
{
    public function __construct(
        public ?string $font      = null,
        public ?string $fontSize  = null,
        public ?string $color     = null,
        public ?bool   $bold      = null,
        public ?string $url       = null,
        public ?bool   $underline = null,
        public ?bool   $strike    = null,
    ) {}
}
