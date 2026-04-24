<?php

declare(strict_types=1);

namespace Lpdf\Canvas;

final readonly class TextStyle
{
    public function __construct(
        public ?string     $font        = null,
        public ?float      $size        = null,
        public ?string     $color       = null,
        public ?TextAlign  $align       = null,
        public ?float      $lineHeight  = null,
        public ?float      $width       = null,
    ) {}
}
