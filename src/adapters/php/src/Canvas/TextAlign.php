<?php

declare(strict_types=1);

namespace Lpdf\Canvas;

enum TextAlign: string
{
    case Left    = 'left';
    case Center  = 'center';
    case Right   = 'right';
    case Justify = 'justify';
}
