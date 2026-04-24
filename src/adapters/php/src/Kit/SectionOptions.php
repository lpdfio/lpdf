<?php

declare(strict_types=1);

namespace Lpdf\Kit;

final readonly class SectionOptions
{
    public function __construct(
        public ?string      $size        = null,
        public ?Orientation $orientation = null,
        public ?string      $margin      = null,
        public ?string      $background  = null,
        public ?string      $title       = null,
        public ?string      $debug       = null,
    ) {}
}
