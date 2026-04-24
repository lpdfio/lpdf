<?php

declare(strict_types=1);

namespace Lpdf\Canvas;

/** @internal Use Canvas::circle() to construct. */
final readonly class CircleNode extends Node
{
    public function __construct(
        private float         $cx,
        private float         $cy,
        private float         $r,
        private ?EllipseStyle $style = null,
    ) {}

    public function jsonSerialize(): mixed
    {
        $attrs = ['x' => (string)$this->cx, 'y' => (string)$this->cy, 'r' => (string)$this->r];
        if ($this->style?->fill        !== null) $attrs['fill']         = $this->style->fill;
        if ($this->style?->stroke      !== null) $attrs['stroke']       = $this->style->stroke;
        if ($this->style?->strokeWidth !== null) $attrs['stroke-width'] = (string)$this->style->strokeWidth;
        if ($this->style?->strokeDash  !== null) $attrs['stroke-dash']  = implode(' ', $this->style->strokeDash);
        return ['type' => 'canvas-circle', 'attrs' => (object) $attrs];
    }
}
