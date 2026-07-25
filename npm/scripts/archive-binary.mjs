#!/usr/bin/env node
/**
 * Archive a release binary for GitHub Release assets.
 *
 * Usage:
 *   node npm/scripts/archive-binary.mjs \
 *     --target aarch64-apple-darwin \
 *     --binary path/to/vue-vet \
 *     --out dist/release
 */
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { PLATFORMS } from '../vue-vet/lib/platforms.js';

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
  const out = args.out;
  if (!target || !binary || !out) {
    throw new Error('Required: --target --binary --out');
  }
  const entry = PLATFORMS.find((item) => item.rustTarget === target);
  if (!entry) {
    throw new Error(`Unsupported rust target: ${target}`);
  }
  if (!fs.existsSync(binary)) {
    throw new Error(`Binary not found: ${binary}`);
  }

  fs.mkdirSync(out, { recursive: true });
  const staging = path.join(out, `staging-${target}`);
  fs.rmSync(staging, { recursive: true, force: true });
  fs.mkdirSync(staging, { recursive: true });
  const staged = path.join(staging, entry.bin);
  fs.copyFileSync(binary, staged);
  if (entry.os !== 'win32') {
    fs.chmodSync(staged, 0o755);
  }

  const archiveBase = `vue-vet-${target}`;
  if (entry.os === 'win32') {
    const zipPath = path.join(out, `${archiveBase}.zip`);
    fs.rmSync(zipPath, { force: true });
    // Use PowerShell Compress-Archive on Windows runners; zip elsewhere.
    if (process.platform === 'win32') {
      execFileSync(
        'powershell.exe',
        ['-NoLogo', '-Command', `Compress-Archive -Path '${staged}' -DestinationPath '${zipPath}'`],
        { stdio: 'inherit' },
      );
    } else {
      execFileSync('zip', ['-j', zipPath, staged], { stdio: 'inherit' });
    }
    console.log(zipPath);
  } else {
    const tarPath = path.join(out, `${archiveBase}.tar.gz`);
    fs.rmSync(tarPath, { force: true });
    execFileSync('tar', ['-C', staging, '-czf', tarPath, entry.bin], { stdio: 'inherit' });
    console.log(tarPath);
  }

  fs.rmSync(staging, { recursive: true, force: true });
}

main();
