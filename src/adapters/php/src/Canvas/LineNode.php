<?php

declare(strict_types=1);

namespace Lpdf\Canvas;

/** @internal Use Canvas::line() to construct. */
final readonly class LineNode extends Node
{
    public function __construct(
        private float      $x1,
        private float      $y1,
        private float      $x2,
        private float      $y2,
        private ?LineStyle $style = null,
    ) {}

    public function jsonSerialize(): mixed
    {
        $attrs = ['x1' => (string)$this->x1, 'y1' => (string)$this->y1, 'x2' => (string)$this->x2, 'y2' => (string)$this->y2];
        if ($this->style?->stroke      !== null) $attrs['stroke']       = $this->style->stroke;
        if ($this->style?->strokeWidth !== null) $attrs['stroke-width'] = (string)$this->style->strokeWidth;
        if ($this->style?->strokeDash  !== null) $attrs['stroke-dash']  = implode(' ', $this->style->strokeDash);
        if ($this->style?->lineCap     !== null) $attrs['line-cap']     = $this->style->lineCap->value;
        if ($this->style?->lineJoin    !== null) $attrs['line-join']    = $this->style->lineJoin->value;
        return ['type' => 'canvas-line', 'attrs' => (object) $attrs];
    }
}
