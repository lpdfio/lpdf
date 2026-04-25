<?php

declare(strict_types=1);

require_once __DIR__ . '/../../../../vendor/autoload.php';

/**
 * example-data.php — render data-invoice.xml with dynamic data from data-invoice.json.
 *
 * Run (after 'make build-adapter-php'):
 *   docker run --rm \
 *     -v "$(pwd)/src/adapters/php/example:/app/src/adapters/php/example" \
 *     -v "$(pwd)/example:/app/example" \
 *     -w /app lpdf-php php src/adapters/php/example/example-data.php
 *
 * Output: example/result/data-invoice-php.pdf
 */

use Lpdf\LpdfEngine;

$root       = __DIR__ . '/../../../../example/';
$xmlFile    = $root . 'xml/data-invoice.xml';
$jsonFile   = $root . 'xml/data-invoice.json';
$outputFile = 'example-data-php.pdf';

$xml  = file_get_contents($xmlFile);
$data = json_decode(file_get_contents($jsonFile), associative: true);

$engine = new LpdfEngine('');  // empty key → free tier (watermark)

$pdf = $engine->renderPdf($xml, data: $data);

file_put_contents($root . "result/{$outputFile}", $pdf);
echo "output: $outputFile (" . number_format(strlen($pdf)) . " bytes)\n";
