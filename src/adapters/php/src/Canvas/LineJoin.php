<?php

declare(strict_types=1);

namespace Lpdf\Canvas;

enum LineJoin: string
{
    case Miter = 'miter';
    case Round = 'round';
    case Bevel = 'bevel';
}
