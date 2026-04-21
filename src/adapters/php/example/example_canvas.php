<?php

declare(strict_types=1);

require_once __DIR__ . '/../../../../vendor/autoload.php';

use Lpdf\LpdfCanvas;
use Lpdf\LpdfEngine;
use Lpdf\LpdfMeta;
use Lpdf\CanvasPageOptions;
use Lpdf\CanvasTextStyle;
use Lpdf\CanvasRectStyle;
use Lpdf\CanvasLineStyle;
use Lpdf\CanvasEllipseStyle;
use Lpdf\CanvasPathStyle;
use Lpdf\CanvasLayerOptions;
use Lpdf\CanvasClip;
use Lpdf\CanvasTransform;
use Lpdf\CanvasRun;

$root = __DIR__ . '/../../../../example/';

// ── Engine ────────────────────────────────────────────────────────────────────

$licenseKey = ''; // file_get_contents($root . 'test.lic');
$engine = new LpdfEngine($licenseKey);

// Load a font (used for canvas-text nodes that reference it).
$engine->loadFont('montserrat', file_get_contents($root . 'assets/fonts/Montserrat-Regular.ttf'));

// ── Build a canvas document ────────────────────────────────────────────────────
//
// Each page uses absolute x/y coordinates with the origin at the top-left.
// The page is 595 × 842 pt (A4 portrait).

$page1 = LpdfCanvas::page(
    children: [

        // ── Heading bar ──────────────────────────────────────────────────────
        LpdfCanvas::rect(0, 0, 595, 60, new CanvasRectStyle(fill: '#1a3a5c')),

        LpdfCanvas::text(
            x: 28, y: 18,
            content: 'lpdf Canvas Primitives',
            style: new CanvasTextStyle(font: 'Helvetica-Bold', size: 22, color: '#ffffff'),
        ),

        // ── Section: rect ────────────────────────────────────────────────────
        LpdfCanvas::text(28, 80, 'canvas-rect', new CanvasTextStyle(font: 'Helvetica-Bold', size: 11, color: '#555555')),

        // Plain fill
        LpdfCanvas::rect(28, 96, 120, 60, new CanvasRectStyle(fill: '#4a90e2')),

        // Fill + stroke
        LpdfCanvas::rect(164, 96, 120, 60, new CanvasRectStyle(
            fill: '#e8f4fd', stroke: '#2980b9', strokeWidth: 2,
        )),

        // Rounded corners
        LpdfCanvas::rect(300, 96, 120, 60, new CanvasRectStyle(
            fill: '#d5f5e3', stroke: '#27ae60', strokeWidth: 1, borderRadius: 12,
        )),

        // Stroke only
        LpdfCanvas::rect(436, 96, 120, 60, new CanvasRectStyle(
            stroke: '#e74c3c', strokeWidth: 3,
        )),

        // ── Section: line ────────────────────────────────────────────────────
        LpdfCanvas::text(28, 176, 'canvas-line', new CanvasTextStyle(font: 'Helvetica-Bold', size: 11, color: '#555555')),

        // Solid thin
        LpdfCanvas::line(28, 192, 300, 192, new CanvasLineStyle(stroke: '#333333', strokeWidth: 1)),

        // Thick round cap
        LpdfCanvas::line(28, 210, 300, 210, new CanvasLineStyle(stroke: '#8e44ad', strokeWidth: 4, lineCap: 'round')),

        // Dashed
        LpdfCanvas::line(28, 228, 300, 228, new CanvasLineStyle(stroke: '#e67e22', strokeWidth: 2, strokeDash: [6, 3])),

        // Diagonal
        LpdfCanvas::line(340, 192, 567, 240, new CanvasLineStyle(stroke: '#16a085', strokeWidth: 2)),

        // ── Section: ellipse / circle ────────────────────────────────────────
        LpdfCanvas::text(28, 256, 'canvas-ellipse / canvas-circle', new CanvasTextStyle(font: 'Helvetica-Bold', size: 11, color: '#555555')),

        // Ellipse filled
        LpdfCanvas::ellipse(100, 305, 72, 40, new CanvasEllipseStyle(fill: '#f39c12', stroke: '#d68910', strokeWidth: 2)),

        // Circle filled
        LpdfCanvas::circle(260, 305, 40, new CanvasEllipseStyle(fill: '#27ae60')),

        // Circle stroke only
        LpdfCanvas::circle(380, 305, 40, new CanvasEllipseStyle(stroke: '#c0392b', strokeWidth: 3)),

        // Ellipse no fill, dashed stroke
        LpdfCanvas::ellipse(490, 305, 65, 35, new CanvasEllipseStyle(stroke: '#2c3e50', strokeWidth: 1, strokeDash: [4, 2])),

        // ── Section: path ────────────────────────────────────────────────────
        LpdfCanvas::text(28, 356, 'canvas-path', new CanvasTextStyle(font: 'Helvetica-Bold', size: 11, color: '#555555')),

        // Triangle
        LpdfCanvas::path('M 28 410 L 128 370 L 228 410 Z', new CanvasPathStyle(fill: '#8e44ad', stroke: '#6c3483', strokeWidth: 1)),

        // Open path (chevron)
        LpdfCanvas::path('M 250 410 L 310 375 L 370 410', new CanvasPathStyle(stroke: '#2980b9', strokeWidth: 3, lineCap: 'round', lineJoin: 'round')),

        // Bezier curve (cubic)
        LpdfCanvas::path('M 400 410 C 420 365 500 365 520 410', new CanvasPathStyle(stroke: '#16a085', strokeWidth: 2, fill: '#d1f2eb')),

        // ── Section: text ────────────────────────────────────────────────────
        LpdfCanvas::text(28, 436, 'canvas-text', new CanvasTextStyle(font: 'Helvetica-Bold', size: 11, color: '#555555')),

        // Left-aligned (default)
        LpdfCanvas::text(28, 454, 'Left-aligned text (Helvetica 12)', new CanvasTextStyle(font: 'Helvetica', size: 12, color: '#222222')),

        // Centered
        LpdfCanvas::text(28, 474, 'Centered over 539 pt', new CanvasTextStyle(
            font: 'Helvetica', size: 12, color: '#2980b9', align: 'center', width: 539,
        )),

        // Right-aligned
        LpdfCanvas::text(28, 494, 'Right-aligned over 539 pt', new CanvasTextStyle(
            font: 'Helvetica', size: 12, color: '#8e44ad', align: 'right', width: 539,
        )),

        // Custom font
        LpdfCanvas::text(28, 518, 'Montserrat Regular — custom TTF font', new CanvasTextStyle(
            font: 'montserrat', size: 13, color: '#1a3a5c',
        )),

        // Rich-text runs
        LpdfCanvas::text(
            x: 28, y: 542,
            content: 'Mixed runs: ',
            style: new CanvasTextStyle(font: 'Helvetica', size: 12, color: '#333333'),
            runs: [
                new CanvasRun('normal '),
                new CanvasRun('bold style', font: 'Helvetica-Bold', color: '#e74c3c'),
                new CanvasRun(' and larger', size: 16, color: '#27ae60'),
            ],
        ),

        // ── Section: layer ───────────────────────────────────────────────────
        LpdfCanvas::text(28, 570, 'canvas-layer', new CanvasTextStyle(font: 'Helvetica-Bold', size: 11, color: '#555555')),

        // Background for the layer demo
        LpdfCanvas::rect(28, 586, 539, 80, new CanvasRectStyle(fill: '#eaf2ff', stroke: '#aed6f1', strokeWidth: 1)),
        LpdfCanvas::text(38, 596, 'Background text (behind semi-transparent layer)', new CanvasTextStyle(font: 'Helvetica', size: 10, color: '#999999')),

        // Semi-transparent red overlay layer
        LpdfCanvas::layer(
            children: [
                LpdfCanvas::rect(28, 586, 539, 80, new CanvasRectStyle(fill: '#e74c3c')),
                LpdfCanvas::text(38, 614, 'Layer at 40% opacity', new CanvasTextStyle(font: 'Helvetica-Bold', size: 14, color: '#ffffff')),
            ],
            options: new CanvasLayerOptions(opacity: 0.4),
        ),

        // Layer with clip
        LpdfCanvas::text(28, 680, 'Layer with clip rect:', new CanvasTextStyle(font: 'Helvetica-Bold', size: 11, color: '#555555')),
        LpdfCanvas::layer(
            children: [
                LpdfCanvas::rect(28, 696, 200, 80, new CanvasRectStyle(fill: '#f9e79f', stroke: '#f1c40f', strokeWidth: 2)),
                LpdfCanvas::ellipse(128, 736, 90, 30, new CanvasEllipseStyle(fill: '#f39c12')),
            ],
            options: new CanvasLayerOptions(clip: new CanvasClip(40, 700, 160, 60, borderRadius: 8)),
        ),

        // Layer with transform (translate + rotate)
        LpdfCanvas::text(260, 680, 'Layer with transform (rotate 15°):', new CanvasTextStyle(font: 'Helvetica-Bold', size: 11, color: '#555555')),
        LpdfCanvas::layer(
            children: [
                LpdfCanvas::rect(0, 0, 120, 40, new CanvasRectStyle(fill: '#d7bde2', stroke: '#8e44ad', strokeWidth: 1, borderRadius: 6)),
                LpdfCanvas::text(8, 12, 'Rotated layer', new CanvasTextStyle(font: 'Helvetica', size: 11, color: '#4a235a')),
            ],
            // Affine [a,b,c,d,e,f]: rotate 15° around (380,720)
            options: new CanvasLayerOptions(
                transform: new CanvasTransform([
                    cos(deg2rad(15)), sin(deg2rad(15)),
                    -sin(deg2rad(15)), cos(deg2rad(15)),
                    380.0, 720.0,
                ]),
            ),
        ),

        // ── Footer rule ──────────────────────────────────────────────────────
        LpdfCanvas::line(28, 808, 567, 808, new CanvasLineStyle(stroke: '#cccccc', strokeWidth: 0.5)),
        LpdfCanvas::text(28, 818, 'generated with lpdf.io', new CanvasTextStyle(font: 'Helvetica', size: 9, color: '#aaaaaa')),
    ],
    options: new CanvasPageOptions(width: 595, height: 842),
);

// ── Assemble & render ─────────────────────────────────────────────────────────

$doc = LpdfCanvas::document(
    pages: [$page1],
    meta: new LpdfMeta(title: 'lpdf Canvas Primitives', author: 'lpdf.io'),
    fonts: [
        'Helvetica'      => ['core' => 'Helvetica'],
        'Helvetica-Bold' => ['core' => 'Helvetica-Bold'],
        'montserrat'     => ['ref'  => 'montserrat'],
    ],
);

$pdf = $engine->renderPdf($doc);

$outputFile = 'example-canvas-php.pdf';
file_put_contents($root . "result/{$outputFile}", $pdf);

echo "output: $outputFile (" . number_format(strlen($pdf)) . " bytes)\n";
