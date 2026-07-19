import * as esbuild from 'esbuild';
import { readFileSync } from 'fs';

const outfile = 'dist/widget/widget.js';

const result = await esbuild.build({
  entryPoints: ['apps/widget/loader/loader.ts'],
  bundle: true,
  format: 'iife',
  minify: true,
  target: 'es2019',
  outfile,
  metafile: true,
});

const stats = readFileSync(outfile);
const bytes = stats.length;

if (bytes > 10240) {
  console.error(`FAIL: loader bundle is ${bytes} bytes (limit: 10240)`);
  process.exit(1);
}

console.log(`OK: loader bundle is ${bytes} bytes (limit: 10240)`);
