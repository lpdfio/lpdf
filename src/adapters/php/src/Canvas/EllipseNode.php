<?php

declare(strict_types=1);

namespace Lpdf\Canvas;

/** @internal Use Canvas::ellipse() to construct. */
final readonly class EllipseNode extends Node
{
    public function __construct(
        private float         $cx,
        private float         $cy,
        private float         $rx,
        private float         $ry,
        private ?EllipseStyle $style = null,
    ) {}

    public function jsonSerialize(): mixed
    {
        $attrs = ['x' => (string)$this->cx, 'y' => (string)$this->cy, 'rx' => (string)$this->rx, 'ry' => (string)$this->ry];
        if ($this->style?->fill        !== null) $attrs['fill']         = $this->style->fill;
        if ($this->style?->stroke      !== null) $attrs['stroke']       = $this->style->stroke;
        if ($this->style?->strokeWidth !== null) $attrs['stroke-width'] = (string)$this->style->strokeWidth;
        if ($this->style?->strokeDash  !== null) $attrs['stroke-dash']  = implode(' ', $this->style->strokeDash);
        return ['type' => 'canvas-ellipse', 'attrs' => (object) $attrs];
    }
}
