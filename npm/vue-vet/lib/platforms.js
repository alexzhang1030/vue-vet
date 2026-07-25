'use strict';

/**
 * Supported native targets for the first release matrix.
 * Maps Rust target triples to npm platform package names and binary names.
 */
const PLATFORMS = Object.freeze([
  {
    rustTarget: 'x86_64-unknown-linux-gnu',
    package: '@vue-vet/linux-x64',
    os: 'linux',
    cpu: 'x64',
    bin: 'vue-vet',
  },
  {
    rustTarget: 'aarch64-unknown-linux-gnu',
    package: '@vue-vet/linux-arm64',
    os: 'linux',
    cpu: 'arm64',
    bin: 'vue-vet',
  },
  {
    rustTarget: 'x86_64-apple-darwin',
    package: '@vue-vet/darwin-x64',
    os: 'darwin',
    cpu: 'x64',
    bin: 'vue-vet',
  },
  {
    rustTarget: 'aarch64-apple-darwin',
    package: '@vue-vet/darwin-arm64',
    os: 'darwin',
    cpu: 'arm64',
    bin: 'vue-vet',
  },
  {
    rustTarget: 'x86_64-pc-windows-msvc',
    package: '@vue-vet/win32-x64',
    os: 'win32',
    cpu: 'x64',
    bin: 'vue-vet.exe',
  },
]);

/**
 * @param {string} platform process.platform
 * @param {string} arch process.arch
 * @returns {{ rustTarget: string, package: string, os: string, cpu: string, bin: string } | null}
 */
function resolvePlatform(platform, arch) {
  return PLATFORMS.find((entry) => entry.os === platform && entry.cpu === arch) ?? null;
}

/**
 * @returns {Record<string, string>}
 */
function optionalDependencies(version) {
  /** @type {Record<string, string>} */
  const deps = {};
  for (const entry of PLATFORMS) {
    deps[entry.package] = version;
  }
  return deps;
}

module.exports = {
  PLATFORMS,
  resolvePlatform,
  optionalDependencies,
};
