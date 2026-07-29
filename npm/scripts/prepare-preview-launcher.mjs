#!/usr/bin/env node
/**
 * Copy the npm launcher into dist/ for pkg.pr.new without mutating the git tree.
 *
 * Usage:
 *   node npm/scripts/prepare-preview-launcher.mjs --version 0.1.16 --out dist/npm/@vue-vet/cli
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { optionalDependencies } from '../vue-vet/lib/platforms.js';

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
  const version = args.version;
  const out = args.out;
  if (!version || !out) {
    throw new Error('Required: --version --out');
  }

  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
  const source = path.join(root, 'npm/vue-vet');
  const outDir = path.resolve(out);
  fs.rmSync(outDir, { recursive: true, force: true });
  fs.mkdirSync(outDir, { recursive: true });

  for (const entry of ['bin', 'lib', 'README.md', 'package.json']) {
    const from = path.join(source, entry);
    const to = path.join(outDir, entry);
    fs.cpSync(from, to, { recursive: true });
  }

  const pkgPath = path.join(outDir, 'package.json');
  const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
  pkg.version = version;
  pkg.optionalDependencies = optionalDependencies(version);
  // Preview publishes ship from dist/; keep repository metadata for compact URLs.
  fs.writeFileSync(pkgPath, `${JSON.stringify(pkg, null, 2)}\n`);
  console.log(`prepared @vue-vet/cli@${version} -> ${outDir}`);
}

main();
