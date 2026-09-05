'use strict';

const { describe, it, before, after } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { resolveBinary } = require('../lib/resolve.js');

describe('resolveBinary', () => {
  /** @type {string} */
  let root;
  /** @type {string} */
  let requireFrom;

  before(() => {
    root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'vue-vet-resolve-')));
    const pkgDir = path.join(root, 'node_modules', '@vue-vet', 'darwin-arm64');
    fs.mkdirSync(path.join(pkgDir, 'bin'), { recursive: true });
    fs.writeFileSync(
      path.join(pkgDir, 'package.json'),
      JSON.stringify({
        name: '@vue-vet/darwin-arm64',
        version: '0.1.0',
        os: ['darwin'],
        cpu: ['arm64'],
      }),
    );
    fs.writeFileSync(path.join(pkgDir, 'bin', 'vue-vet'), '#!/bin/sh\necho ok\n', {
      mode: 0o755,
    });
    // A fake consumer module so createRequire resolves from here.
    requireFrom = path.join(root, 'consumer.js');
    fs.writeFileSync(requireFrom, '');
  });

  after(() => {
    fs.rmSync(root, { recursive: true, force: true });
  });

  it('resolves the native binary from the optional platform package', () => {
    const binary = resolveBinary({
      platform: 'darwin',
      arch: 'arm64',
      requireFrom,
    });
    const expected = path.join(
      root,
      'node_modules',
      '@vue-vet',
      'darwin-arm64',
      'bin',
      'vue-vet',
    );
    assert.equal(fs.realpathSync(binary), fs.realpathSync(expected));
    assert.ok(fs.existsSync(binary));
  });

  it('errors clearly for unsupported platforms', () => {
    assert.throws(
      () => resolveBinary({ platform: 'aix', arch: 'ppc64', requireFrom }),
      /does not ship a prebuilt binary/,
    );
  });

  it('errors clearly when the optional package is missing', () => {
    assert.throws(
      () =>
        resolveBinary({
          platform: 'linux',
          arch: 'x64',
          requireFrom,
        }),
      /Could not find optional dependency @vue-vet\/linux-x64/,
    );
  });
});
