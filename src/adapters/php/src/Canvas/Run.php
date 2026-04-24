<?php

declare(strict_types=1);

namespace Lpdf\Canvas;

/** A rich-text run for {@see \Lpdf\Canvas::text()}. */
final readonly class Run
{
    public function __construct(
        public string  $text,
        public ?string $font  = null,
        public ?float  $size  = null,
        public ?string $color = null,
    ) {}
}
