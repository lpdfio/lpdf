<?php

declare(strict_types=1);

namespace Lpdf\Canvas;

/** @internal Use Canvas::path() to construct. */
final readonly class PathNode extends Node
{
    public function __construct(
        private string     $d,
        private ?PathStyle $style = null,
    ) {}

    public function jsonSerialize(): mixed
    {
        $attrs = ['d' => $this->d];
        if ($this->style?->fill            !== null) $attrs['fill']            = $this->style->fill;
        if ($this->style?->stroke          !== null) $attrs['stroke']          = $this->style->stroke;
        if ($this->style?->strokeWidth     !== null) $attrs['stroke-width'] = (string)$this->style->strokeWidth;
        if ($this->style?->strokeDash      !== null) $attrs['stroke-dash']  = implode(' ', $this->style->strokeDash);
        if ($this->style?->fillRuleEvenodd !== null) $attrs['fill-rule']    = $this->style->fillRuleEvenodd ? 'evenodd' : 'nonzero';
        if ($this->style?->lineCap         !== null) $attrs['line-cap']     = $this->style->lineCap->value;
        if ($this->style?->lineJoin        !== null) $attrs['line-join']    = $this->style->lineJoin->value;
        return ['type' => 'canvas-path', 'attrs' => (object) $attrs];
    }
}
