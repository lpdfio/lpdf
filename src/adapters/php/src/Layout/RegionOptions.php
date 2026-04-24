<?php

declare(strict_types=1);

namespace Lpdf\Layout;

use Lpdf\Shared\PageScope;

final readonly class RegionOptions
{
    public function __construct(
        public PageScope|string|null $page  = null,
        public ?string $w     = null,
        public ?string $debug = null,
    ) {}
}
