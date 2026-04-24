<?php

declare(strict_types=1);

namespace Lpdf\Canvas;

final readonly class Clip
{
    public function __construct(
        public float  $x,
        public float  $y,
        public float  $w,
        public float  $h,
        public float  $borderRadius = 0.0,
    ) {}
}
