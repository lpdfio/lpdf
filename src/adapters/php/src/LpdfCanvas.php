<?php

declare(strict_types=1);

namespace Lpdf;

use Lpdf\Canvas\CircleNode;
use Lpdf\Canvas\EllipseNode;
use Lpdf\Canvas\EllipseStyle;
use Lpdf\Canvas\ImageNode;
use Lpdf\Canvas\LayerNode;
use Lpdf\Canvas\LayerOptions;
use Lpdf\Canvas\LineNode;
use Lpdf\Canvas\LineStyle;
use Lpdf\Canvas\Node;
use Lpdf\Canvas\PathNode;
use Lpdf\Canvas\PathStyle;
use Lpdf\Canvas\RectNode;
use Lpdf\Canvas\RectStyle;
use Lpdf\Canvas\Run;
use Lpdf\Canvas\TextNode;
use Lpdf\Canvas\TextStyle;

/**
 * Static factory for coordinate/absolute canvas primitives.
 *
 * Canvas uses a coordinate-based rendering model: x/y positions are absolute,
 * with the origin at the top-left of the page (y increases downward).
 *
 * Pass layers to {@see Kit::canvas()}, then include that block in {@see Kit::section()}.
 */
final class LpdfCanvas
{
    // ── Primitive helpers ─────────────────────────────────────────────────────

    /**
     * A text node at the given coordinates.
     *
     * @param Run[] $runs Optional rich-text runs (override content per run).
     */
    public static function text(
        float      $x,
        float      $y,
        string     $content,
        ?TextStyle $style = null,
        array      $runs  = [],
    ): TextNode {
        return new TextNode($x, $y, $content, $style, $runs);
    }

    /** A rectangle. */
    public static function rect(
        float      $x,
        float      $y,
        float      $w,
        float      $h,
        ?RectStyle $style = null,
    ): RectNode {
        return new RectNode($x, $y, $w, $h, $style);
    }

    /** A straight line from (x1,y1) to (x2,y2). */
    public static function line(
        float      $x1,
        float      $y1,
        float      $x2,
        float      $y2,
        ?LineStyle $style = null,
    ): LineNode {
        return new LineNode($x1, $y1, $x2, $y2, $style);
    }

    /** An ellipse centred at (cx, cy) with radii rx and ry. */
    public static function ellipse(
        float         $cx,
        float         $cy,
        float         $rx,
        float         $ry,
        ?EllipseStyle $style = null,
    ): EllipseNode {
        return new EllipseNode($cx, $cy, $rx, $ry, $style);
    }

    /** A circle centred at (cx, cy) with radius r. */
    public static function circle(
        float         $cx,
        float         $cy,
        float         $r,
        ?EllipseStyle $style = null,
    ): CircleNode {
        return new CircleNode($cx, $cy, $r, $style);
    }

    /** A path described by an SVG path data string. */
    public static function path(
        string     $d,
        ?PathStyle $style = null,
    ): PathNode {
        return new PathNode($d, $style);
    }

    /**
     * An image placed at (x, y) with dimensions (w × h).
     *
     * `$name` must match a key registered via {@see \Lpdf\Engine::loadImage()}.
     */
    public static function img(
        float  $x,
        float  $y,
        float  $w,
        float  $h,
        string $name,
    ): ImageNode {
        return new ImageNode($x, $y, $w, $h, $name);
    }

    // ── Layer helper ──────────────────────────────────────────────────────────

    /**
     * A layer that groups canvas primitives, optionally applying a page scope,
     * opacity, a transform, or a clip region.
     *
     * Layers are the direct children of a section's canvas block. Primitives must
     * live inside a layer — bare primitives at the canvas level are not allowed.
     *
     * @param Node[] $nodes
     */
    public static function layer(
        array         $nodes,
        ?LayerOptions $options = null,
    ): LayerNode {
        return new LayerNode($nodes, $options);
    }
}
