import { LpdfEngine } from '../dist/node/lpdf.js';

// unlicensed — watermark should appear
const free = new LpdfEngine('');
const freeResult = JSON.parse(free.render('<doc/>'));
console.log('unlicensed:', freeResult.watermark);

// licensed — watermark should be null
const paid = new LpdfEngine('test-key');
const paidResult = JSON.parse(paid.render('<doc/>'));
console.log('licensed:  ', paidResult.watermark);
