#!/usr/bin/env node
/**
 * Sync npm/vue-vet/package.json version and optionalDependencies to VERSION.
 *
 * Usage: node npm/scripts/sync-launcher-version.mjs 0.1.0
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { optionalDependencies } from '../vue-vet/lib/platforms.js';

const version = process.argv[2];
if (!version) {
  throw new Error('Usage: sync-launcher-version.mjs <version>');
}

const pkgPath = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  '../vue-vet/package.json',
);
const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
pkg.version = version;
pkg.optionalDependencies = optionalDependencies(version);
fs.writeFileSync(pkgPath, `${JSON.stringify(pkg, null, 2)}\n`);
console.log(`synced vue-vet@${version}`);
