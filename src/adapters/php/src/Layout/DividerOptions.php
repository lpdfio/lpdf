<?php

declare(strict_types=1);

namespace Lpdf\Layout;

final readonly class DividerOptions
{
    public function __construct(
        public ?string $color     = null,
        public ?string $thickness = null,
        public ?string $direction = null,
    ) {}
}
