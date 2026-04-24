<?php

declare(strict_types=1);

namespace Lpdf\Layout;

final readonly class LinkOptions
{
    public function __construct(
        public ?string $url    = null,
        public ?string $width  = null,
        public ?string $height = null,
    ) {}
}
