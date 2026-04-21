<?php

declare(strict_types=1);

namespace Lpdf;

/**
 * Static factory helpers for constructing canvas document trees.
 *
 * All helpers return plain serialisable objects. Pass the result of
 * {@see document()} to {@see LpdfEngine::renderPdf()}.
 *
 * Canvas uses a coordinate-based rendering model: x/y positions are absolute,
 * with the origin at the top-left of the page (y increases downward).
 *
 * @example
 * ```php
 * use Lpdf\{LpdfCanvas, LpdfEngine, CanvasRectStyle};
 *
 * $doc = LpdfCanvas::document(
 *     pages: [
 *         LpdfCanvas::page(
 *             children: [
 *                 LpdfCanvas::rect(40, 40, 200, 100, new CanvasRectStyle(fill: '#4a90e2')),
 *                 LpdfCanvas::text(40, 160, 'Hello Canvas!'),
 *             ],
 *             options: new CanvasPageOptions(width: 595, height: 842),
 *         ),
 *     ],
 * );
 * $pdf = (new LpdfEngine(''))->renderPdf($doc);
 * ```
 */
final class LpdfCanvas
{
    // ── Primitive helpers ─────────────────────────────────────────────────────

    /**
     * A text node at the given coordinates.
     *
     * @param CanvasRun[] $runs Optional rich-text runs (override content per run).
     */
    public static function text(
        float            $x,
        float            $y,
        string           $content,
        ?CanvasTextStyle $style = null,
        array            $runs  = [],
    ): LpdfCanvasTextNode {
        return new LpdfCanvasTextNode($x, $y, $content, $style, $runs);
    }

    /** A rectangle. */
    public static function rect(
        float            $x,
        float            $y,
        float            $w,
        float            $h,
        ?CanvasRectStyle $style = null,
    ): LpdfCanvasRectNode {
        return new LpdfCanvasRectNode($x, $y, $w, $h, $style);
    }

    /** A straight line from (x1,y1) to (x2,y2). */
    public static function line(
        float            $x1,
        float            $y1,
        float            $x2,
        float            $y2,
        ?CanvasLineStyle $style = null,
    ): LpdfCanvasLineNode {
        return new LpdfCanvasLineNode($x1, $y1, $x2, $y2, $style);
    }

    /** An ellipse centred at (cx, cy) with radii rx and ry. */
    public static function ellipse(
        float               $cx,
        float               $cy,
        float               $rx,
        float               $ry,
        ?CanvasEllipseStyle $style = null,
    ): LpdfCanvasEllipseNode {
        return new LpdfCanvasEllipseNode($cx, $cy, $rx, $ry, $style);
    }

    /** A circle centred at (cx, cy) with radius r. */
    public static function circle(
        float               $cx,
        float               $cy,
        float               $r,
        ?CanvasEllipseStyle $style = null,
    ): LpdfCanvasCircleNode {
        return new LpdfCanvasCircleNode($cx, $cy, $r, $style);
    }

    /** A path described by an SVG path data string. */
    public static function path(
        string           $d,
        ?CanvasPathStyle $style = null,
    ): LpdfCanvasPathNode {
        return new LpdfCanvasPathNode($d, $style);
    }

    /**
     * An image placed at (x, y) with dimensions (w × h).
     *
     * `$src` must match a key registered via {@see LpdfEngine::loadImage()} or
     * declared in the document's `$images` asset map.
     */
    public static function image(
        float  $x,
        float  $y,
        float  $w,
        float  $h,
        string $src,
    ): LpdfCanvasImageNode {
        return new LpdfCanvasImageNode($x, $y, $w, $h, $src);
    }

    // ── Layer helper ──────────────────────────────────────────────────────────

    /**
     * A layer that groups children, optionally applying opacity, a transform,
     * or a clip region.
     *
     * @param LpdfCanvasNode[] $children
     */
    public static function layer(
        array               $children,
        ?CanvasLayerOptions $options = null,
    ): LpdfCanvasLayerNode {
        return new LpdfCanvasLayerNode($children, $options);
    }

    // ── Page + document ───────────────────────────────────────────────────────

    /**
     * A canvas page.
     *
     * @param LpdfCanvasNode[] $children
     */
    public static function page(
        array              $children,
        ?CanvasPageOptions $options = null,
    ): LpdfCanvasPage {
        return new LpdfCanvasPage($children, $options);
    }

    /**
     * Build the root canvas document, ready for {@see LpdfEngine::renderPdf()}.
     *
     * @param LpdfCanvasPage[] $pages
     * @param ?LpdfMeta        $meta
     * @param array<string, array{core?: string, ref?: string}> $fonts
     *   Asset font declarations. Example:
     *   `['Helvetica' => ['core' => 'Helvetica'], 'MyFont' => ['ref' => 'my-font']]`
     * @param array<string, array{ref?: string}>                $images
     *   Asset image declarations. Example: `['logo' => ['ref' => 'logo']]`
     */
    public static function document(
        array     $pages,
        ?LpdfMeta $meta   = null,
        array     $fonts  = [],
        array     $images = [],
    ): LpdfCanvasDocument {
        return new LpdfCanvasDocument($pages, $meta, $fonts, $images);
    }
}
