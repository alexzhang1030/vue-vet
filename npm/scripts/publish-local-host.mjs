#!/usr/bin/env node
/**
 * Publish the host platform package + launcher for a local smoke / name claim.
 * Requires npm auth with rights to `vue-vet` and `@vue-vet`.
 *
 * Usage: node npm/scripts/publish-local-host.mjs [--dry-run]
 */
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { resolvePlatform } from '../vue-vet/lib/platforms.js';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const dryRun = process.argv.includes('--dry-run');

function run(cmd, args, opts = {}) {
  console.log(`$ ${cmd} ${args.join(' ')}`);
  execFileSync(cmd, args, { stdio: 'inherit', cwd: root, ...opts });
}

function main() {
  const entry = resolvePlatform(process.platform, process.arch);
  if (!entry) {
    throw new Error(`Unsupported host ${process.platform}-${process.arch}`);
  }
  const osCpu = `${entry.os}-${entry.cpu}`;
  const pkgDir = path.join(root, 'dist/npm/@vue-vet', osCpu);
  if (!fs.existsSync(path.join(pkgDir, 'package.json'))) {
    throw new Error(`Missing packed platform package at ${pkgDir}. Run: just pack-platform`);
  }

  run('node', ['npm/scripts/sync-launcher-version.mjs', '0.1.0']);

  const publishArgs = ['publish', '--access', 'public'];
  if (dryRun) {
    publishArgs.push('--dry-run');
  }

  // Prefer absolute paths so npm does not interpret `npm/vue-vet` as a git host.
  run('npm', [...publishArgs, '--registry', 'https://registry.npmjs.org/', pkgDir]);
  run('npm', [
    ...publishArgs,
    '--registry',
    'https://registry.npmjs.org/',
    path.join(root, 'npm/vue-vet'),
  ]);
}

main();
