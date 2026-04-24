<?php

declare(strict_types=1);

namespace Lpdf\Kit;

enum Orientation: string
{
    case Portrait  = 'portrait';
    case Landscape = 'landscape';
}
