<?php

declare(strict_types=1);

namespace Lpdf\Tests;

use Lpdf\LpdfEngine;
use Lpdf\LpdfKit;
use Lpdf\LpdfTokens;
use Lpdf\DocumentOptions;
use PHPUnit\Framework\TestCase;

final class SnapshotTest extends TestCase
{
    /** @return array<string, array{string}> */
    public static function fixtureProvider(): array
    {
        return SnapshotHelper::fixtureProvider();
    }

    #[\PHPUnit\Framework\Attributes\DataProvider('fixtureProvider')]
    public function testFixtureMatchesStoredHash(string $name): void
    {
        $xml    = file_get_contents(SnapshotHelper::fixtures() . "/$name.xml");
        $engine = new LpdfEngine('test-key');
        $bytes  = $engine->renderPdf($xml);
        SnapshotHelper::compareOrUpdate($name, $bytes);
    }

    public function testOutputIsPdf(): void
    {
        $xml   = file_get_contents(SnapshotHelper::fixtures() . '/example1.xml');
        $bytes = (new LpdfEngine('test-key'))->renderPdf($xml);
        self::assertStringStartsWith('%PDF-', $bytes);
    }

    public function testCustomFontDoesNotThrow(): void
    {
        $xml    = file_get_contents(SnapshotHelper::fixtures() . '/example1.xml');
        $engine = new LpdfEngine('test-key');

        // Load a placeholder font (empty bytes will be ignored by the core if
        // the document does not reference it; we just assert no exception).
        $engine->loadFont('TestFont', '');
        $bytes = $engine->renderPdf($xml);
        self::assertStringStartsWith('%PDF-', $bytes);
    }

    public function testKitToXmlReturnsXmlDeclaration(): void
    {
        $doc = LpdfKit::document([LpdfKit::page([])]);
        $xml = LpdfEngine::kitToXml($doc);
        self::assertStringStartsWith('<?xml version="1.0"', $xml);
    }

    public function testKitToXmlContainsLpdfRoot(): void
    {
        $doc = LpdfKit::document([LpdfKit::page([])]);
        $xml = LpdfEngine::kitToXml($doc);
        self::assertStringContainsString('<lpdf version="1">', $xml);
    }

    public function testKitToXmlBuiltinFontInAssets(): void
    {
        $doc = LpdfKit::document([], new DocumentOptions(
            tokens: new LpdfTokens(fonts: ['heading' => ['builtin' => 'Helvetica-Bold']]),
        ));
        $xml = LpdfEngine::kitToXml($doc);
        self::assertStringContainsString('<assets>', $xml);
        self::assertStringContainsString('core="Helvetica-Bold"', $xml);
        // Font must NOT appear inside <tokens>
        $tokensStart = strpos($xml, '<tokens>');
        $tokensEnd   = strpos($xml, '</tokens>');
        $fontsInTokens = strpos($xml, '<fonts>', $tokensStart ?: 0);
        self::assertTrue(
            $tokensStart === false || $fontsInTokens === false || $fontsInTokens > $tokensEnd,
            'Font was incorrectly placed inside <tokens>',
        );
    }

    public function testKitToXmlCustomFontUsesRefAlias(): void
    {
        $doc = LpdfKit::document([], new DocumentOptions(
            tokens: new LpdfTokens(fonts: ['body' => ['src' => '/fonts/MyFont.ttf']]),
        ));
        $xml = LpdfEngine::kitToXml($doc);
        self::assertStringContainsString('ref="body"', $xml);
        self::assertStringNotContainsString('src=', $xml);
    }

    public function testKitToXmlProducedXmlRendersToValidPdf(): void
    {
        $doc = LpdfKit::document([
            LpdfKit::page([LpdfKit::text(['Hello from kitToXml'])]),
        ]);
        $xml    = LpdfEngine::kitToXml($doc);
        $engine = new LpdfEngine('test-key');
        $bytes  = $engine->renderPdf($xml);
        self::assertStringStartsWith('%PDF-', $bytes);
    }

    public function testSetEncryptionProducesEncryptedPdf(): void
    {
        $xml = file_get_contents(SnapshotHelper::fixtures() . '/example1.xml');
        $engine = new LpdfEngine('test-key');
        $engine->setEncryption('', 's3cr3t');
        $bytes = $engine->renderPdf($xml);
        self::assertStringStartsWith('%PDF-', $bytes);
        // Encrypted PDFs contain the /Encrypt dictionary entry
        self::assertStringContainsString('/Encrypt', $bytes);
    }

    public function testLoadImageDoesNotThrowAndProducesValidPdf(): void
    {
        // A minimal 1×1 white grayscale PNG
        $png1x1 = hex2bin(
            '89504e470d0a1a0a0000000d49484452000000010000000108000000003a7e9b55' .
            '0000000a49444154789c6260000000020001e221bc330000000049454e44ae426082',
        );
        $xml    = file_get_contents(SnapshotHelper::fixtures() . '/example1.xml');
        $engine = new LpdfEngine('test-key');
        $engine->loadImage('testimg', $png1x1);
        $bytes  = $engine->renderPdf($xml);
        self::assertStringStartsWith('%PDF-', $bytes);
    }
}
