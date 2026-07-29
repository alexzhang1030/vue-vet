#!/usr/bin/env node
/**
 * Print `[workspace.package].version` from the root Cargo.toml (no Rust toolchain).
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const cargoToml = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../Cargo.toml');
const text = fs.readFileSync(cargoToml, 'utf8');
const marker = '[workspace.package]';
const index = text.indexOf(marker);
if (index === -1) {
  throw new Error('missing [workspace.package] in Cargo.toml');
}
const match = text.slice(index).match(/^version\s*=\s*"([^"]+)"/m);
if (!match) {
  throw new Error('missing version under [workspace.package]');
}
process.stdout.write(match[1]);
