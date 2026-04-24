<?php

declare(strict_types=1);

namespace Lpdf\Layout;

enum Pin: string
{
    case Top    = 'top';
    case Bottom = 'bottom';
    case Left   = 'left';
    case Right  = 'right';
}
