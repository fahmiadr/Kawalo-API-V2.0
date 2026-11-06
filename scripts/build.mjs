import esbuild from 'esbuild';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, '..');
const distDir = path.resolve(rootDir, 'dist');

if (!fs.existsSync(distDir)) {
  fs.mkdirSync(distDir, { recursive: true });
}

console.log('===============================================================');
console.log('  Kawalo Mobile API - Production Node.js Bundler (esbuild)    ');
console.log('===============================================================');

const startTime = performance.now();

try {
  await esbuild.build({
    entryPoints: [path.resolve(rootDir, 'src/server.ts')],
    bundle: true,
    platform: 'node',
    target: 'node24',
    format: 'esm',
    outfile: path.resolve(distDir, 'server.js'),
    sourcemap: true,
    packages: 'external',
    banner: {
      js: '// Kawalo Mobile API - Production Bundle\nimport { createRequire } from "module"; const require = createRequire(import.meta.url);',
    },
    logLevel: 'info',
  });

  const duration = ((performance.now() - startTime) / 1000).toFixed(2);
  const stat = fs.statSync(path.resolve(distDir, 'server.js'));
  const sizeKb = (stat.size / 1024).toFixed(2);

  console.log(`\n✔ Successfully built dist/server.js in ${duration}s (${sizeKb} KB)`);
  console.log('Run in production with: node dist/server.js\n');
} catch (err) {
  console.error('\n❌ Build failed:', err);
  process.exit(1);
}
