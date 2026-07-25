#!/usr/bin/env node
/**
 * Build one @vue-vet/{os}-{arch} npm package directory from a compiled binary.
 *
 * Usage:
 *   node npm/scripts/pack-platform.mjs \
 *     --target aarch64-apple-darwin \
 *     --binary path/to/vue-vet \
 *     --version 0.1.0 \
 *     --out dist/npm/@vue-vet/darwin-arm64
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { PLATFORMS } from '../vue-vet/lib/platforms.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, '../..');

function parseArgs(argv) {
  /** @type {Record<string, string>} */
  const args = {};
  for (let i = 0; i < argv.length; i += 1) {
    const key = argv[i];
    if (!key.startsWith('--')) {
      throw new Error(`Unexpected argument: ${key}`);
    }
    const name = key.slice(2);
    const value = argv[i + 1];
    if (value === undefined || value.startsWith('--')) {
      throw new Error(`Missing value for --${name}`);
    }
    args[name] = value;
    i += 1;
  }
  return args;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const target = args.target;
  const binary = args.binary;
  const version = args.version;
  const out = args.out;
  if (!target || !binary || !version || !out) {
    throw new Error('Required: --target --binary --version --out');
  }

  const entry = PLATFORMS.find((item) => item.rustTarget === target);
  if (!entry) {
    throw new Error(`Unsupported rust target: ${target}`);
  }
  if (!fs.existsSync(binary)) {
    throw new Error(`Binary not found: ${binary}`);
  }

  const outDir = path.resolve(out);
  const binDir = path.join(outDir, 'bin');
  fs.rmSync(outDir, { recursive: true, force: true });
  fs.mkdirSync(binDir, { recursive: true });

  const templatePath = path.join(root, 'npm/template-platform/package.json.template');
  const template = fs.readFileSync(templatePath, 'utf8');
  const osCpu = `${entry.os}-${entry.cpu}`;
  const packageJson = template
    .replaceAll('__OS_CPU__', osCpu)
    .replaceAll('__VERSION__', version)
    .replaceAll('__OS__', entry.os)
    .replaceAll('__CPU__', entry.cpu)
    .replaceAll('__BIN__', entry.bin);
  fs.writeFileSync(path.join(outDir, 'package.json'), packageJson);

  const destBinary = path.join(binDir, entry.bin);
  fs.copyFileSync(binary, destBinary);
  if (entry.os !== 'win32') {
    fs.chmodSync(destBinary, 0o755);
  }

  console.log(`packed ${entry.package}@${version} -> ${outDir}`);
}

main();
