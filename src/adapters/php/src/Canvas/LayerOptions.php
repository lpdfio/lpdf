<?php

declare(strict_types=1);

namespace Lpdf\Canvas;

use Lpdf\Shared\PageScope;

final readonly class LayerOptions
{
    /**
     * @param PageScope|string|null $page  Named scope or numeric range, e.g. '2-4', '1,3-5'.
     */
    public function __construct(
        public PageScope|string|null $page      = null,
        public ?float                $opacity   = null,
        public ?Transform            $transform = null,
        public ?Clip                 $clip      = null,
    ) {}
}
