<?php

declare(strict_types=1);

namespace Lpdf\Canvas;

/** @internal Use Canvas::rect() to construct. */
final readonly class RectNode extends Node
{
    public function __construct(
        private float      $x,
        private float      $y,
        private float      $w,
        private float      $h,
        private ?RectStyle $style = null,
    ) {}

    public function jsonSerialize(): mixed
    {
        $attrs = ['x' => (string)$this->x, 'y' => (string)$this->y, 'w' => (string)$this->w, 'h' => (string)$this->h];
        if ($this->style?->fill         !== null) $attrs['fill']         = $this->style->fill;
        if ($this->style?->stroke       !== null) $attrs['stroke']       = $this->style->stroke;
        if ($this->style?->strokeWidth  !== null) $attrs['stroke-width'] = (string)$this->style->strokeWidth;
        if ($this->style?->strokeDash   !== null) $attrs['stroke-dash']  = implode(' ', $this->style->strokeDash);
        if ($this->style?->borderRadius !== null) $attrs['radius']       = (string)$this->style->borderRadius;
        return ['type' => 'canvas-rect', 'attrs' => (object) $attrs];
    }
}
