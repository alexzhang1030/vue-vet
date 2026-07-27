'use strict';

const path = require('node:path');
const { describe, it } = require('node:test');
const assert = require('node:assert/strict');
const { findWorkspaceBinary, resolveLauncher } = require('../lib/cli');

describe('cli launcher resolution', () => {
  it('uses the configured path when set', async () => {
    const configured = '/tmp/custom-vue-vet';
    const launcher = await resolveLauncher(configured, '/workspace', {
      accessImpl: async () => {},
      commandExistsImpl: async () => false,
    });
    assert.deepEqual(launcher, { command: configured, argsPrefix: [] });
  });

  it('prefers workspace debug binary over PATH and npx', async () => {
    const root = '/repo';
    const debug = path.join(root, 'target', 'debug', 'vue-vet');
    const launcher = await resolveLauncher('', root, {
      accessImpl: async (candidate) => {
        if (candidate === debug) {
          return;
        }
        throw Object.assign(new Error('missing'), { code: 'ENOENT' });
      },
      commandExistsImpl: async () => true,
    });
    assert.deepEqual(launcher, { command: debug, argsPrefix: [] });
  });

  it('falls back to PATH when no workspace binary exists', async () => {
    const launcher = await resolveLauncher('', '/repo', {
      accessImpl: async () => {
        throw Object.assign(new Error('missing'), { code: 'ENOENT' });
      },
      commandExistsImpl: async (name) => name === 'vue-vet',
    });
    assert.deepEqual(launcher, { command: 'vue-vet', argsPrefix: [] });
  });

  it('falls back to npx when PATH has no vue-vet', async () => {
    const launcher = await resolveLauncher('', '/repo', {
      accessImpl: async () => {
        throw Object.assign(new Error('missing'), { code: 'ENOENT' });
      },
      commandExistsImpl: async () => false,
    });
    assert.deepEqual(launcher, {
      command: 'npx',
      argsPrefix: ['--yes', '@vue-vet/cli'],
    });
  });

  it('findWorkspaceBinary checks debug before release', async () => {
    const root = '/repo';
    const debug = path.join(root, 'target', 'debug', 'vue-vet');
    const release = path.join(root, 'target', 'release', 'vue-vet');
    const seen = [];
    const found = await findWorkspaceBinary(root, {
      accessImpl: async (candidate) => {
        seen.push(candidate);
        if (candidate === release) {
          return;
        }
        throw Object.assign(new Error('missing'), { code: 'ENOENT' });
      },
    });
    assert.equal(found, release);
    assert.ok(seen.includes(debug));
    assert.ok(seen.indexOf(debug) < seen.indexOf(release));
  });
});
