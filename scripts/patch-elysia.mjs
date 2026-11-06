import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, '..');

const filesToPatch = [
  path.resolve(rootDir, 'node_modules/@elysiajs/node/dist/index.mjs'),
  path.resolve(rootDir, 'node_modules/@elysiajs/node/dist/cjs/index.js'),
];

for (const filePath of filesToPatch) {
  if (!fs.existsSync(filePath)) {
    console.warn(`[patch-elysia] Target file not found: ${filePath}`);
    continue;
  }

  let content = fs.readFileSync(filePath, 'utf8');

  // Check if already patched
  if (content.includes('const raw = message.rawData;')) {
    console.log(`[patch-elysia] File already patched: ${path.basename(filePath)}`);
    continue;
  }

  // Replace context.message(ws, message.text());
  const target = 'context.message(ws, message.text());';
  const replacement = `const raw = message.rawData;
        const isBinary = Buffer.isBuffer(raw) || raw instanceof Uint8Array || raw instanceof ArrayBuffer;
        context.message(ws, isBinary ? raw : message.text());`;

  if (content.includes(target)) {
    content = content.replace(target, replacement);
    fs.writeFileSync(filePath, content, 'utf8');
    console.log(`[patch-elysia] Successfully patched: ${path.basename(filePath)}`);
  } else {
    console.warn(`[patch-elysia] Target string not found in ${path.basename(filePath)}`);
  }
}
