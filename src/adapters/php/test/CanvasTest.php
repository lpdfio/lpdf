<?php

declare(strict_types=1);

namespace Lpdf\Tests;

use Lpdf\CanvasClip;
use Lpdf\CanvasEllipseStyle;
use Lpdf\CanvasLayerOptions;
use Lpdf\CanvasPageOptions;
use Lpdf\CanvasPathStyle;
use Lpdf\CanvasRectStyle;
use Lpdf\CanvasRun;
use Lpdf\CanvasTextStyle;
use Lpdf\CanvasTransform;
use Lpdf\LpdfCanvas;
use Lpdf\LpdfCanvasDocument;
use Lpdf\LpdfEngine;
use Lpdf\LpdfMeta;
use PHPUnit\Framework\TestCase;

final class CanvasTest extends TestCase
{
    // ── Integration: engine produces a valid PDF ──────────────────────────────

    public function testCanvasOutputIsPdf(): void
    {
        $doc = $this->minimalDoc();
        $bytes = (new LpdfEngine('test-key'))->renderPdf($doc);
        self::assertStringStartsWith('%PDF-', $bytes);
    }

    public function testCanvasSnapshotMatchesOrIsCreated(): void
    {
        $doc   = $this->comprehensiveDoc();
        $bytes = (new LpdfEngine('test-key'))->renderPdf($doc);
        self::assertStringStartsWith('%PDF-', $bytes);
        SnapshotHelper::compareOrUpdate('canvas_comprehensive', $bytes);
    }

    // ── Serialisation unit tests ──────────────────────────────────────────────

    public function testDocumentSerializesToCanvasDocument(): void
    {
        $doc = LpdfCanvas::document(pages: [LpdfCanvas::page([])]);
        $json = json_decode(json_encode($doc, JSON_THROW_ON_ERROR), true);

        self::assertSame(1, $json['version']);
        self::assertSame('canvas-document', $json['type']);
        self::assertArrayHasKey('pages', $json);
    }

    public function testPageSerializesToCanvasPage(): void
    {
        $page = LpdfCanvas::page([], new CanvasPageOptions(width: 595, height: 842));
        $json = json_decode(json_encode($page, JSON_THROW_ON_ERROR), true);

        self::assertSame('canvas-page', $json['type']);
        self::assertSame(595.0, (float) $json['attrs']['width']);
        self::assertSame(842.0, (float) $json['attrs']['height']);
    }

    public function testRectSerializesCorrectly(): void
    {
        $node = LpdfCanvas::rect(10, 20, 100, 50, new CanvasRectStyle(fill: '#ff0000', borderRadius: 5));
        $json = json_decode(json_encode($node, JSON_THROW_ON_ERROR), true);

        self::assertSame('canvas-rect', $json['type']);
        self::assertSame(10.0, (float) $json['attrs']['x']);
        self::assertSame(20.0, (float) $json['attrs']['y']);
        self::assertSame(100.0, (float) $json['attrs']['w']);
        self::assertSame(50.0, (float) $json['attrs']['h']);
        self::assertSame('#ff0000', $json['attrs']['fill']);
        self::assertSame(5.0, (float) $json['attrs']['borderRadius']);
    }

    public function testLineSerializesCorrectly(): void
    {
        $node = LpdfCanvas::line(0, 0, 100, 100);
        $json = json_decode(json_encode($node, JSON_THROW_ON_ERROR), true);

        self::assertSame('canvas-line', $json['type']);
        self::assertSame(0.0, (float) $json['attrs']['x1']);
        self::assertSame(0.0, (float) $json['attrs']['y1']);
        self::assertSame(100.0, (float) $json['attrs']['x2']);
        self::assertSame(100.0, (float) $json['attrs']['y2']);
    }

    public function testEllipseSerializesCorrectly(): void
    {
        $node = LpdfCanvas::ellipse(50, 50, 40, 20, new CanvasEllipseStyle(fill: '#00ff00'));
        $json = json_decode(json_encode($node, JSON_THROW_ON_ERROR), true);

        self::assertSame('canvas-ellipse', $json['type']);
        self::assertSame(50.0, (float) $json['attrs']['cx']);
        self::assertSame(50.0, (float) $json['attrs']['cy']);
        self::assertSame(40.0, (float) $json['attrs']['rx']);
        self::assertSame(20.0, (float) $json['attrs']['ry']);
        self::assertSame('#00ff00', $json['attrs']['fill']);
    }

    public function testCircleSerializesToCanvasCircle(): void
    {
        $node = LpdfCanvas::circle(100, 100, 30);
        $json = json_decode(json_encode($node, JSON_THROW_ON_ERROR), true);

        self::assertSame('canvas-circle', $json['type']);
        self::assertSame(100.0, (float) $json['attrs']['cx']);
        self::assertSame(100.0, (float) $json['attrs']['cy']);
        self::assertSame(30.0, (float) $json['attrs']['r']);
    }

    public function testPathSerializesCorrectly(): void
    {
        $node = LpdfCanvas::path('M 0 0 L 100 100 Z', new CanvasPathStyle(fill: '#0000ff', fillRuleEvenodd: true));
        $json = json_decode(json_encode($node, JSON_THROW_ON_ERROR), true);

        self::assertSame('canvas-path', $json['type']);
        self::assertSame('M 0 0 L 100 100 Z', $json['attrs']['d']);
        self::assertSame('#0000ff', $json['attrs']['fill']);
        self::assertTrue($json['attrs']['fillRuleEvenodd']);
    }

    public function testImageSerializesCorrectly(): void
    {
        $node = LpdfCanvas::image(10, 20, 200, 150, 'logo');
        $json = json_decode(json_encode($node, JSON_THROW_ON_ERROR), true);

        self::assertSame('canvas-image', $json['type']);
        self::assertSame('logo', $json['attrs']['src']);
        self::assertSame(200.0, (float) $json['attrs']['w']);
    }

    public function testTextSerializesCorrectly(): void
    {
        $node = LpdfCanvas::text(20, 40, 'Hello', new CanvasTextStyle(font: 'Helvetica', size: 14, color: '#333333'));
        $json = json_decode(json_encode($node, JSON_THROW_ON_ERROR), true);

        self::assertSame('canvas-text', $json['type']);
        self::assertSame('Hello', $json['attrs']['content']);
        self::assertSame('Helvetica', $json['attrs']['font']);
        self::assertSame(14.0, (float) $json['attrs']['size']);
        self::assertSame('#333333', $json['attrs']['color']);
        self::assertArrayNotHasKey('runs', $json);
    }

    public function testTextWithRunsSerializesRuns(): void
    {
        $node = LpdfCanvas::text(
            x: 0, y: 0, content: 'base',
            runs: [new CanvasRun('bold', font: 'Helvetica-Bold', color: '#ff0000')],
        );
        $json = json_decode(json_encode($node, JSON_THROW_ON_ERROR), true);

        self::assertArrayHasKey('runs', $json);
        self::assertCount(1, $json['runs']);
        self::assertSame('bold', $json['runs'][0]['text']);
        self::assertSame('Helvetica-Bold', $json['runs'][0]['font']);
        self::assertSame('#ff0000', $json['runs'][0]['color']);
    }

    public function testLayerSerializesWithOpacity(): void
    {
        $node = LpdfCanvas::layer(
            children: [LpdfCanvas::rect(0, 0, 100, 100)],
            options: new CanvasLayerOptions(opacity: 0.5),
        );
        $json = json_decode(json_encode($node, JSON_THROW_ON_ERROR), true);

        self::assertSame('canvas-layer', $json['type']);
        self::assertSame(0.5, $json['attrs']['opacity']);
        self::assertCount(1, $json['children']);
    }

    public function testLayerSerializesWithClip(): void
    {
        $node = LpdfCanvas::layer(
            children: [],
            options: new CanvasLayerOptions(clip: new CanvasClip(10, 10, 100, 50, 5)),
        );
        $json = json_decode(json_encode($node, JSON_THROW_ON_ERROR), true);

        self::assertSame(10.0, (float) $json['attrs']['clip']['x']);
        self::assertSame(100.0, (float) $json['attrs']['clip']['w']);
        self::assertSame(5.0, (float) $json['attrs']['clip']['borderRadius']);
    }

    public function testLayerSerializesWithTransform(): void
    {
        $matrix = [1.0, 0.0, 0.0, 1.0, 50.0, 100.0];
        $node = LpdfCanvas::layer(
            children: [],
            options: new CanvasLayerOptions(transform: new CanvasTransform($matrix)),
        );
        $json = json_decode(json_encode($node, JSON_THROW_ON_ERROR), true);

        self::assertEquals($matrix, $json['attrs']['transform']);
    }

    public function testNullStyleAttrsAreOmitted(): void
    {
        $node = LpdfCanvas::rect(0, 0, 50, 50); // no style
        $json = json_decode(json_encode($node, JSON_THROW_ON_ERROR), true);

        self::assertArrayNotHasKey('fill', $json['attrs']);
        self::assertArrayNotHasKey('stroke', $json['attrs']);
    }

    public function testDocumentWithMetaSerializesTitle(): void
    {
        $doc = LpdfCanvas::document(
            pages: [LpdfCanvas::page([])],
            meta: new LpdfMeta(title: 'Test Doc', author: 'Tester'),
        );
        $json = json_decode(json_encode($doc, JSON_THROW_ON_ERROR), true);

        self::assertSame('Test Doc', $json['attrs']['meta']['title']);
        self::assertSame('Tester', $json['attrs']['meta']['author']);
    }

    public function testDocumentWithFontsAndImagesSerializesAssets(): void
    {
        $doc = LpdfCanvas::document(
            pages: [LpdfCanvas::page([])],
            fonts: ['Helvetica' => ['core' => 'Helvetica']],
            images: ['logo' => ['ref' => 'logo']],
        );
        $json = json_decode(json_encode($doc, JSON_THROW_ON_ERROR), true);

        self::assertArrayHasKey('assets', $json['attrs']);
        self::assertSame('Helvetica', $json['attrs']['assets']['fonts']['Helvetica']['core']);
        self::assertSame('logo', $json['attrs']['assets']['images']['logo']['ref']);
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    private function minimalDoc(): LpdfCanvasDocument
    {
        return LpdfCanvas::document(
            pages: [
                LpdfCanvas::page(
                    children: [
                        LpdfCanvas::rect(40, 40, 200, 100, new CanvasRectStyle(fill: '#4a90e2')),
                        LpdfCanvas::text(40, 160, 'Hello Canvas!', new CanvasTextStyle(font: 'Helvetica', size: 16, color: '#000000')),
                    ],
                    options: new CanvasPageOptions(width: 595, height: 842),
                ),
            ],
            fonts: ['Helvetica' => ['core' => 'Helvetica']],
        );
    }

    private function comprehensiveDoc(): LpdfCanvasDocument
    {
        return LpdfCanvas::document(
            pages: [
                LpdfCanvas::page(
                    children: [
                        LpdfCanvas::rect(40, 40, 200, 100, new CanvasRectStyle(fill: '#4a90e2', stroke: '#1a5276', strokeWidth: 2, borderRadius: 8)),
                        LpdfCanvas::line(40, 170, 555, 170),
                        LpdfCanvas::ellipse(140, 250, 80, 50, new CanvasEllipseStyle(fill: '#f39c12')),
                        LpdfCanvas::circle(400, 250, 60, new CanvasEllipseStyle(fill: '#27ae60')),
                        LpdfCanvas::path('M 40 360 L 200 310 L 360 360 Z', new CanvasPathStyle(fill: '#8e44ad')),
                        LpdfCanvas::text(40, 420, 'Canvas text', new CanvasTextStyle(font: 'Helvetica', size: 18, color: '#1a1a1a')),
                        LpdfCanvas::layer(
                            children: [LpdfCanvas::rect(40, 460, 515, 60, new CanvasRectStyle(fill: '#e74c3c'))],
                            options: new CanvasLayerOptions(opacity: 0.5),
                        ),
                    ],
                    options: new CanvasPageOptions(width: 595, height: 842),
                ),
            ],
            fonts: [
                'Helvetica' => ['core' => 'Helvetica'],
            ],
        );
    }
}
