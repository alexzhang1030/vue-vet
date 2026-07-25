'use strict';

const { describe, it } = require('node:test');
const assert = require('node:assert/strict');
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
