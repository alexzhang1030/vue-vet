'use strict';

const { describe, it } = require('node:test');
const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {
  PLATFORMS,
  resolvePlatform,
  optionalDependencies,
} = require('../lib/platforms.js');

describe('resolvePlatform', () => {
  it('maps every supported host pair', () => {
    for (const entry of PLATFORMS) {
      const resolved = resolvePlatform(entry.os, entry.cpu);
      assert.equal(resolved?.package, entry.package);
      assert.equal(resolved?.rustTarget, entry.rustTarget);
    }
  });

  it('returns null for unsupported hosts', () => {
    assert.equal(resolvePlatform('freebsd', 'x64'), null);
    assert.equal(resolvePlatform('linux', 'ia32'), null);
    assert.equal(resolvePlatform('win32', 'arm64'), null);
  });
});
describe('optionalDependencies', () => {
  it('pins every platform package to the given version', () => {
    const deps = optionalDependencies('0.1.0');
    assert.equal(Object.keys(deps).length, PLATFORMS.length);
    for (const entry of PLATFORMS) {
      assert.equal(deps[entry.package], '0.1.0');
    }
  });
});

describe('pack-platform contract', () => {
  it('emits platform packages without bin and keeps the stub bytes', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vue-vet-pack-'));
    const stub = path.join(root, 'stub.bin');
    const stubBytes = Buffer.from('vue-vet-stub\n');
    const packer = path.resolve(__dirname, '../../scripts/pack-platform.mjs');
    try {
      fs.writeFileSync(stub, stubBytes);
      for (const entry of PLATFORMS) {
        const out = path.join(root, `${entry.os}-${entry.cpu}`);
        execFileSync(process.execPath, [
          packer,
          '--target',
          entry.rustTarget,
          '--binary',
          stub,
          '--version',
          '0.0.0-test',
          '--out',
          out,
        ]);
        const pkg = JSON.parse(fs.readFileSync(path.join(out, 'package.json'), 'utf8'));
        assert.equal(pkg.bin, undefined, `${pkg.name} must not declare bin`);
        assert.deepEqual(pkg.files, ['bin'], `${pkg.name} files must list bin`);
        const native = path.join(out, 'bin', entry.bin);
        assert.ok(fs.existsSync(native), `${pkg.name} must keep ${entry.bin}`);
        assert.deepEqual(fs.readFileSync(native), stubBytes, `${pkg.name} native bytes must match the stub`);
      }
      const launcher = JSON.parse(fs.readFileSync(path.join(__dirname, '../package.json'), 'utf8'));
      assert.deepEqual(launcher.bin, { 'vue-vet': 'bin/vue-vet.js' });
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });
});
