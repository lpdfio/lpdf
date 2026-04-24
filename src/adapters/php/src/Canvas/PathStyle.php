<?php

declare(strict_types=1);

namespace Lpdf\Canvas;

final readonly class PathStyle
{
    public function __construct(
        public ?string      $fill              = null,
        public ?string      $stroke            = null,
        public ?float       $strokeWidth       = null,
        /** @var float[]|null */
        public ?array       $strokeDash        = null,
        public ?bool        $fillRuleEvenodd   = null,
        public ?LineCap     $lineCap           = null,
        public ?LineJoin    $lineJoin          = null,
    ) {}
}
