<?php

declare(strict_types=1);

namespace Lpdf\Canvas;

final readonly class Transform
{
    /**
     * Affine transform matrix [a, b, c, d, e, f] (SVG / PDF convention).
     * @param float[] $matrix
     */
    public function __construct(public array $matrix) {}

    // ── Factory helpers ───────────────────────────────────────────────────────

    /**
     * Rotate by $degrees clockwise around the origin (or around ($cx, $cy) if given).
     *
     * The rotation is applied first; the optional translation shifts the origin
     * before rotating, so the result rotates around the given point.
     */
    public static function rotate(float $degrees, float $cx = 0.0, float $cy = 0.0): self
    {
        $rad = deg2rad($degrees);
        $cos = cos($rad);
        $sin = sin($rad);
        // Translate so (cx,cy) becomes the origin, rotate, then translate back.
        $e = $cx - $cx * $cos + $cy * $sin;
        $f = $cy - $cx * $sin - $cy * $cos;
        return new self([$cos, $sin, -$sin, $cos, $e, $f]);
    }

    /** Scale uniformly by $factor (or independently by $sx / $sy). */
    public static function scale(float $sx, ?float $sy = null): self
    {
        $sy ??= $sx;
        return new self([$sx, 0.0, 0.0, $sy, 0.0, 0.0]);
    }

    /** Translate by ($tx, $ty). */
    public static function translate(float $tx, float $ty): self
    {
        return new self([1.0, 0.0, 0.0, 1.0, $tx, $ty]);
    }

    /**
     * Combine two transforms: apply $other first, then $this.
     * Equivalent to matrix multiplication: $this × $other.
     */
    public function then(self $other): self
    {
        [$a1, $b1, $c1, $d1, $e1, $f1] = $this->matrix;
        [$a2, $b2, $c2, $d2, $e2, $f2] = $other->matrix;
        return new self([
            $a1 * $a2 + $c1 * $b2,
            $b1 * $a2 + $d1 * $b2,
            $a1 * $c2 + $c1 * $d2,
            $b1 * $c2 + $d1 * $d2,
            $a1 * $e2 + $c1 * $f2 + $e1,
            $b1 * $e2 + $d1 * $f2 + $f1,
        ]);
    }
}
