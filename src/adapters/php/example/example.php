<?php

declare(strict_types=1);

require_once __DIR__ . '/../../../../vendor/autoload.php';

use Lpdf\LpdfEngine;

// init engine
$engine = new LpdfEngine('');       // empty key → free tier (watermark)

// optional: load fonts and assets
// $engine->loadFont('Inter', file_get_contents('/path/to/Inter.ttf'));

$inputFile  = 'invoice.xml';
$outputFile = 'invoice-php.pdf';

$root = __DIR__ . '/../../../../example/';

// load xml from file
$xml = file_get_contents($root . $inputFile);

// render pdf from xml
$pdf = $engine->renderPdf($xml);

// write pdf to file
file_put_contents($root . $outputFile, $pdf);

echo "output: $outputFile (" . number_format(strlen($pdf)) . " bytes)\n";
