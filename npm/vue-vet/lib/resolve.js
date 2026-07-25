'use strict';

const fs = require('node:fs');
const path = require('node:path');
const { createRequire } = require('node:module');
const { resolvePlatform } = require('./platforms.js');

/**
 * Resolve the absolute path to the native vue-vet binary for this host.
 *
 * @param {{
 *   platform?: string,
 *   arch?: string,
 *   requireFrom?: string,
 * }} [options]
 * @returns {string}
 */
function resolveBinary(options = {}) {
  const platform = options.platform ?? process.platform;
  const arch = options.arch ?? process.arch;
  const requireFrom = options.requireFrom ?? __filename;
  const entry = resolvePlatform(platform, arch);
  if (entry === null) {
    const supported = 'linux-x64, linux-arm64, darwin-x64, darwin-arm64, win32-x64';
    throw new Error(
      `vue-vet does not ship a prebuilt binary for ${platform}-${arch}. ` +
        `Supported platforms: ${supported}. ` +
        'Build from source with Rust: https://github.com/alexzhang1030/vue-vet#development',
    );
  }

  const require = createRequire(requireFrom);
  let packageRoot;
  try {
    packageRoot = path.dirname(require.resolve(`${entry.package}/package.json`));
  } catch (cause) {
    throw new Error(
      `Could not find optional dependency ${entry.package}. ` +
        'Reinstall vue-vet (remove node_modules and the lockfile entry if needed), ' +
        `or install ${entry.package} explicitly. ` +
        'See https://github.com/alexzhang1030/vue-vet/blob/main/docs/install.md',
      { cause },
    );
  }

  const binaryPath = path.join(packageRoot, 'bin', entry.bin);
  if (!fs.existsSync(binaryPath)) {
    throw new Error(
      `Package ${entry.package} is installed but ${entry.bin} is missing at ${binaryPath}.`,
    );
  }
  // GitHub Actions artifacts and some pack pipelines drop the executable bit.
  // Restore it before spawn so optionalDependencies installs stay runnable.
  if (process.platform !== 'win32') {
    try {
      fs.accessSync(binaryPath, fs.constants.X_OK);
    } catch {
      fs.chmodSync(binaryPath, 0o755);
    }
  }
  return binaryPath;
}

module.exports = { resolveBinary };
