<?php

declare(strict_types=1);

namespace Lpdf\Shared;

/**
 * Named page-scope values for {@see \Lpdf\Canvas\LayerOptions::$page} and
 * {@see \Lpdf\Layout\RegionOptions::$page}.
 * Numeric ranges (e.g. '2-4', '1,3-5', '2-last') must still be passed as plain strings.
 */
enum PageScope: string
{
    case Each  = 'each';
    case First = 'first';
    case Last  = 'last';
    case Odd   = 'odd';
    case Even  = 'even';
}
