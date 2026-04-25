<?php
declare(strict_types=1);

namespace Lpdf\Tests;

use Lpdf\LpdfCanvas;
use Lpdf\Canvas\Clip;
use Lpdf\Canvas\EllipseStyle;
use Lpdf\Canvas\LayerOptions;
use Lpdf\Shared\PageScope;
use Lpdf\Canvas\PathStyle;
use Lpdf\Canvas\RectStyle;
use Lpdf\Canvas\Run;
use Lpdf\Canvas\TextStyle;
use Lpdf\Canvas\Transform;
use Lpdf\LpdfKit;
use Lpdf\Kit\DocumentOptions;
use Lpdf\Kit\SectionOptions;
use Lpdf\Kit\DocumentTokens;
use Lpdf\LpdfLayout;
use Lpdf\Kit\Document;
use Lpdf\LpdfEngine;
use PHPUnit\Framework\TestCase;

final class CanvasTest extends TestCase
{
    // â”€â”€ Integration: engine produces a valid PDF â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

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

    // â”€â”€ Document / section serialisation â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    public function testDocumentSerializesToDocument(): void
    {
        $doc = LpdfKit::document(sections: [LpdfKit::section()]);
        $json = json_decode(json_encode($doc, JSON_THROW_ON_ERROR), true);

        self::assertSame(1, $json['version']);
        self::assertSame('document', $json['type']);
        self::assertArrayHasKey('nodes', $json);
    }

    public function testSectionSerializesToSection(): void
    {
        $section = LpdfKit::section(options: new SectionOptions(size: 'a4', margin: '20pt'));
        $json = json_decode(json_encode($section, JSON_THROW_ON_ERROR), true);

        self::assertSame('section', $json['type']);
        self::assertSame('a4', $json['attrs']['size']);
        self::assertSame('20pt', $json['attrs']['margin']);
    }

    public function testSectionWithCanvasLayersSerializesKindNodes(): void
    {
        $layer = LpdfCanvas::layer(nodes: [LpdfCanvas::rect(0, 0, 100, 100)]);
        $section = LpdfKit::section(nodes: [LpdfKit::canvas([$layer])]);
        $json = json_decode(json_encode($section, JSON_THROW_ON_ERROR), true);

        self::assertSame('section', $json['type']);
        self::assertCount(1, $json['nodes']);
        self::assertSame('canvas', $json['nodes'][0]['type']);
        self::assertCount(1, $json['nodes'][0]['nodes']);
    }

    public function testSectionWithBothLayoutAndCanvas(): void
    {
        $layer = LpdfCanvas::layer(nodes: [LpdfCanvas::rect(0, 0, 100, 100)]);
        $section = LpdfKit::section(nodes: [
            LpdfKit::layout([LpdfLayout::text(['Hello'])]),
            LpdfKit::canvas([$layer]),
        ]);
        $json = json_decode(json_encode($section, JSON_THROW_ON_ERROR), true);

        // Layout first, canvas on top (overlay)
        self::assertSame('layout', $json['nodes'][0]['type']);
        self::assertSame('canvas', $json['nodes'][1]['type']);
    }

    public function testSectionWithCanvasUnderlayOrder(): void
    {
        $layer = LpdfCanvas::layer(nodes: [LpdfCanvas::rect(0, 0, 100, 100)]);
        $section = LpdfKit::section(nodes: [
            LpdfKit::canvas([$layer]),
            LpdfKit::layout([LpdfLayout::text(['Hello'])]),
        ]);
        $json = json_decode(json_encode($section, JSON_THROW_ON_ERROR), true);

        self::assertSame('canvas', $json['nodes'][0]['type']);
        self::assertSame('layout', $json['nodes'][1]['type']);
    }

    public function testSectionWithTitleSerializesAttr(): void
    {
        $section = LpdfKit::section(options: new SectionOptions(title: 'Cover'));
        $json = json_decode(json_encode($section, JSON_THROW_ON_ERROR), true);

        self::assertSame('Cover', $json['attrs']['title']);
    }

    // â”€â”€ Region serialisation â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    public function testRegionSerializesToLayoutRegion(): void
    {
        $region = LpdfLayout::region('top-right', [LpdfLayout::text(['Header'])]);
        $json = json_decode(json_encode($region, JSON_THROW_ON_ERROR), true);

        self::assertSame('layout-region', $json['type']);
        self::assertSame('top-right', $json['attrs']['pin']);
        self::assertArrayHasKey('nodes', $json);
        self::assertCount(1, $json['nodes']);
    }

    // â”€â”€ Canvas primitive serialisation â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    public function testRectSerializesCorrectly(): void
    {
        $node = LpdfCanvas::rect(10, 20, 100, 50, new RectStyle(fill: '#ff0000', borderRadius: 5));
        $json = json_decode(json_encode($node, JSON_THROW_ON_ERROR), true);

        self::assertSame('canvas-rect', $json['type']);
        self::assertSame(10.0, (float) $json['attrs']['x']);
        self::assertSame(20.0, (float) $json['attrs']['y']);
        self::assertSame(100.0, (float) $json['attrs']['w']);
        self::assertSame(50.0, (float) $json['attrs']['h']);
        self::assertSame('#ff0000', $json['attrs']['fill']);
        self::assertSame(5.0, (float) $json['attrs']['radius']);
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
        $node = LpdfCanvas::ellipse(50, 50, 40, 20, new EllipseStyle(fill: '#00ff00'));
        $json = json_decode(json_encode($node, JSON_THROW_ON_ERROR), true);

        self::assertSame('canvas-ellipse', $json['type']);
        self::assertSame(50.0, (float) $json['attrs']['x']);
        self::assertSame(50.0, (float) $json['attrs']['y']);
        self::assertSame(40.0, (float) $json['attrs']['rx']);
        self::assertSame(20.0, (float) $json['attrs']['ry']);
        self::assertSame('#00ff00', $json['attrs']['fill']);
    }

    public function testCircleSerializesToCanvasCircle(): void
    {
        $node = LpdfCanvas::circle(100, 100, 30);
        $json = json_decode(json_encode($node, JSON_THROW_ON_ERROR), true);

        self::assertSame('canvas-circle', $json['type']);
        self::assertSame(100.0, (float) $json['attrs']['x']);
        self::assertSame(100.0, (float) $json['attrs']['y']);
        self::assertSame(30.0, (float) $json['attrs']['r']);
    }

    public function testPathSerializesCorrectly(): void
    {
        $node = LpdfCanvas::path('M 0 0 L 100 100 Z', new PathStyle(fill: '#0000ff', fillRuleEvenodd: true));
        $json = json_decode(json_encode($node, JSON_THROW_ON_ERROR), true);

        self::assertSame('canvas-path', $json['type']);
        self::assertSame('M 0 0 L 100 100 Z', $json['attrs']['d']);
        self::assertSame('#0000ff', $json['attrs']['fill']);
        self::assertSame('evenodd', $json['attrs']['fill-rule']);
    }

    public function testImgSerializesToCanvasImg(): void
    {
        $node = LpdfCanvas::img(10, 20, 200, 150, 'logo');
        $json = json_decode(json_encode($node, JSON_THROW_ON_ERROR), true);

        self::assertSame('canvas-img', $json['type']);
        self::assertSame('logo', $json['attrs']['name']);
        self::assertSame(200.0, (float) $json['attrs']['w']);
    }

    public function testTextSerializesCorrectly(): void
    {
        $node = LpdfCanvas::text(20, 40, 'Hello', new TextStyle(font: 'Helvetica', size: 14, color: '#333333'));
        $json = json_decode(json_encode($node, JSON_THROW_ON_ERROR), true);

        self::assertSame('canvas-text', $json['type']);
        self::assertSame('Hello', $json['text']);
        self::assertSame('Helvetica', $json['attrs']['font']);
        self::assertSame(14.0, (float) $json['attrs']['font-size']);
        self::assertSame('#333333', $json['attrs']['color']);
        self::assertArrayNotHasKey('runs', $json);
    }

    public function testTextWithRunsSerializesRuns(): void
    {
        $node = LpdfCanvas::text(
            x: 0, y: 0, content: 'base',
            runs: [new Run('bold', font: 'Helvetica-Bold', color: '#ff0000')],
        );
        $json = json_decode(json_encode($node, JSON_THROW_ON_ERROR), true);

        self::assertArrayHasKey('runs', $json);
        self::assertCount(1, $json['runs']);
        self::assertSame('bold', $json['runs'][0]['text']);
        self::assertSame('Helvetica-Bold', $json['runs'][0]['attrs']['font']);
        self::assertSame('#ff0000', $json['runs'][0]['attrs']['color']);
    }

    // â”€â”€ Layer serialisation â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    public function testLayerSerializesWithOpacity(): void
    {
        $node = LpdfCanvas::layer(
            nodes: [LpdfCanvas::rect(0, 0, 100, 100)],
            options: new LayerOptions(opacity: 0.5),
        );
        $json = json_decode(json_encode($node, JSON_THROW_ON_ERROR), true);

        self::assertSame('canvas-layer', $json['type']);
        self::assertSame('0.5', $json['attrs']['opacity']);
        self::assertCount(1, $json['nodes']);
    }

    public function testLayerWithPageScopeSerializesPage(): void
    {
        $node = LpdfCanvas::layer(
            nodes: [],
            options: new LayerOptions(page: PageScope::Each),
        );
        $json = json_decode(json_encode($node, JSON_THROW_ON_ERROR), true);

        self::assertSame('each', $json['attrs']['page']);
    }

    public function testLayerSerializesWithClip(): void
    {
        $node = LpdfCanvas::layer(
            nodes: [],
            options: new LayerOptions(clip: new Clip(10, 10, 100, 50, 5)),
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
            nodes: [],
            options: new LayerOptions(transform: new Transform($matrix)),
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

    // â”€â”€ Helpers â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    private function minimalDoc(): Document
    {
        return LpdfKit::document(
            sections: [
                LpdfKit::section(
                    nodes: [
                        LpdfKit::canvas([
                            LpdfCanvas::layer(nodes: [
                                LpdfCanvas::rect(40, 40, 200, 100, new RectStyle(fill: '#4a90e2')),
                                LpdfCanvas::text(40, 160, 'Hello Canvas!', new TextStyle(font: 'Helvetica', size: 16, color: '#000000')),
                            ]),
                        ]),
                    ],
                    options: new SectionOptions(size: 'a4'),
                ),
            ],
            options: new DocumentOptions(tokens: new DocumentTokens(fonts: ['Helvetica' => ['builtin' => 'Helvetica']])),
        );
    }

    private function comprehensiveDoc(): Document
    {
        return LpdfKit::document(
            sections: [
                LpdfKit::section(
                    nodes: [
                        LpdfKit::canvas([
                            LpdfCanvas::layer(nodes: [
                                LpdfCanvas::rect(40, 40, 200, 100, new RectStyle(fill: '#4a90e2', stroke: '#1a5276', strokeWidth: 2, borderRadius: 8)),
                                LpdfCanvas::line(40, 170, 555, 170),
                                LpdfCanvas::ellipse(140, 250, 80, 50, new EllipseStyle(fill: '#f39c12')),
                                LpdfCanvas::circle(400, 250, 60, new EllipseStyle(fill: '#27ae60')),
                                LpdfCanvas::path('M 40 360 L 200 310 L 360 360 Z', new PathStyle(fill: '#8e44ad')),
                                LpdfCanvas::text(40, 420, 'Canvas text', new TextStyle(font: 'Helvetica', size: 18, color: '#1a1a1a')),
                            ]),
                            LpdfCanvas::layer(
                                nodes: [LpdfCanvas::rect(40, 460, 515, 60, new RectStyle(fill: '#e74c3c'))],
                                options: new LayerOptions(opacity: 0.5),
                            ),
                        ]),
                    ],
                    options: new SectionOptions(size: 'a4'),
                ),
            ],
            options: new DocumentOptions(tokens: new DocumentTokens(fonts: ['Helvetica' => ['builtin' => 'Helvetica']])),
        );
    }
}
