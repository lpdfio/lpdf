<?php

declare(strict_types=1);

require_once __DIR__ . '/../../../../vendor/autoload.php';

use Lpdf\LpdfEngine;

$root = __DIR__ . '/../../../../example/';

$examples = [
    'example1',
    'example2',
];

// init engine
$licenseKey = ''; //file_get_contents($root . 'test.lic');
$engine = new LpdfEngine($licenseKey);

// load assets (only used if referenced in xml/layout)
$engine->loadFont('montserrat', file_get_contents($root . 'assets/fonts/Montserrat-Regular.ttf'));
$engine->loadImage('logo', file_get_contents($root . 'assets/images/logo-lpdf.png'));

foreach ($examples as $example) {
    // load xml from file
    $xml = file_get_contents($root . "xml/{$example}.xml");
    
    // render pdf from xml
    $pdf = $engine->renderPdf($xml);

    // define output file name
    $outputFile = "{$example}-php.pdf";
    
    // write pdf to output file
    file_put_contents($root . "result/{$outputFile}", $pdf);
    
    // echo 
    echo "output: $outputFile (" . number_format(strlen($pdf)) . " bytes)\n";
}
