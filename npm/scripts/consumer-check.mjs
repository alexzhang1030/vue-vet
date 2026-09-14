#!/usr/bin/env node
/**
 * Existing-artifact npm consumer check: pack a built binary + launcher,
 * install offline into an isolated consumer, and compare installed vs direct.
 *
 * Usage:
 *   node npm/scripts/consumer-check.mjs \
 *     --binary path/to/vue-vet \
 *     --target aarch64-apple-darwin \
 *     --out /tmp/vue-vet-consumer-check
 */
import { execFileSync, spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { PLATFORMS } from '../vue-vet/lib/platforms.js';
import { compareRuns, findDiagnosticRule, sha256File } from './lib/compare.mjs';

const MAX_BUFFER = 64 * 1024 * 1024;
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, '../..');
const npmCmd = process.platform === 'win32' ? 'npm.cmd' : 'npm';

/**
 * @param {string[]} argv
 * @returns {Record<string, string>}
 */
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

/**
 * @param {string} cargoToml
 * @returns {string}
 */
function readWorkspaceVersion(cargoToml) {
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
  return match[1];
}

/**
 * @param {string} filePath
 * @returns {unknown}
 */
function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

/**
 * @param {string} text
 * @returns {unknown}
 */
function parseJsonText(text) {
  const trimmed = text.trim();
  try {
    return JSON.parse(trimmed);
  } catch (error) {
    const startArr = trimmed.indexOf('[');
    const startObj = trimmed.indexOf('{');
    const start =
      startArr === -1 ? startObj : startObj === -1 ? startArr : Math.min(startArr, startObj);
    if (start === -1) {
      throw error;
    }
    return JSON.parse(trimmed.slice(start));
  }
}

/**
 * @param {string[]} args
 * @param {import('node:child_process').SpawnSyncOptions} [options]
 */
function spawnNpm(args, options = {}) {
  const spawnOpts = {
    encoding: 'utf8',
    maxBuffer: MAX_BUFFER,
    shell: false,
    ...options,
  };
  let result = spawnSync(npmCmd, args, spawnOpts);
  if (result.error && process.platform === 'win32') {
    result = spawnSync(npmCmd, args, { ...spawnOpts, shell: true });
  }
  if (result.error) {
    throw result.error;
  }
  return result;
}

/**
 * @param {string} command
 * @param {string[]} args
 * @param {import('node:child_process').SpawnSyncOptions} [options]
 */
function captureRun(command, args, options = {}) {
  const result = spawnSync(command, args, {
    encoding: 'utf8',
    maxBuffer: MAX_BUFFER,
    cwd: root,
    ...options,
  });
  return {
    stdout: result.stdout ?? '',
    stderr: result.stderr ?? (result.error ? result.error.message : ''),
    status: result.status,
  };
}

/**
 * @param {string} rawDir
 * @param {string} name
 * @param {{ stdout?: string, stderr?: string }} run
 */
function writeRaw(rawDir, name, run) {
  fs.writeFileSync(path.join(rawDir, `${name}.stdout`), run.stdout ?? '');
  fs.writeFileSync(path.join(rawDir, `${name}.stderr`), run.stderr ?? '');
}

/**
 * @param {string} stagedDir
 * @param {string} destDir
 */
function packTarball(stagedDir, destDir) {
  const result = spawnNpm(
    ['pack', '--pack-destination', destDir, '--json', stagedDir],
    { cwd: root },
  );
  if (result.status !== 0) {
    throw new Error(
      `npm pack failed (${result.status}): ${(result.stderr || result.stdout).trim()}`,
    );
  }
  const parsed = parseJsonText(result.stdout ?? '');
  const item = Array.isArray(parsed) ? parsed[0] : parsed;
  if (item === null || typeof item !== 'object') {
    throw new Error('npm pack --json did not return a tarball record');
  }
  const record = /** @type {Record<string, unknown>} */ (item);
  if (typeof record.filename !== 'string') {
    throw new Error('npm pack --json missing filename');
  }
  const filename = path.basename(record.filename);
  const tarballPath = path.join(destDir, filename);
  if (!fs.existsSync(tarballPath)) {
    throw new Error(`npm pack did not write ${tarballPath}`);
  }
  return {
    filename,
    path: tarballPath,
    size: fs.statSync(tarballPath).size,
    sha256: sha256File(tarballPath),
    integrity: typeof record.integrity === 'string' ? record.integrity : null,
    shasum: typeof record.shasum === 'string' ? record.shasum : null,
  };
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const binaryArg = args.binary;
  const target = args.target;
  const outArg = args.out;
  if (!binaryArg || !target || !outArg) {
    throw new Error('Required: --binary --target --out');
  }

  const entry = PLATFORMS.find((item) => item.rustTarget === target);
  if (!entry) {
    throw new Error(`Unsupported rust target: ${target}`);
  }

  const binaryPath = path.resolve(binaryArg);
  if (!fs.existsSync(binaryPath) || !fs.statSync(binaryPath).isFile()) {
    throw new Error(`Binary not found: ${binaryPath}`);
  }

  const version = readWorkspaceVersion(path.join(root, 'Cargo.toml'));
  const launcherPkgPath = path.join(root, 'npm/vue-vet/package.json');
  const launcherSourcePkg = /** @type {{ version?: string }} */ (readJson(launcherPkgPath));
  if (launcherSourcePkg.version !== version) {
    throw new Error(
      `npm/vue-vet package.json version ${launcherSourcePkg.version} != workspace version ${version}`,
    );
  }

  const outDir = path.resolve(outArg);
  if (fs.existsSync(outDir)) {
    throw new Error(`--out already exists: ${outDir}`);
  }
  fs.mkdirSync(outDir, { recursive: true });

  const projectPath = path.resolve(args.project ?? path.join(root, 'fixtures/projects/basic'));
  const expectRule = args['expect-rule'] ?? 'no-v-html';
  const startedAt = new Date().toISOString();
  const binarySize = fs.statSync(binaryPath).size;
  const binarySha = sha256File(binaryPath);

  const platformStaging = path.join(outDir, 'staging', 'platform');
  const launcherStaging = path.join(outDir, 'staging', 'launcher');
  const tarballDir = path.join(outDir, 'tarballs');
  const consumerDir = path.join(outDir, 'consumer');
  const rawDir = path.join(outDir, 'raw');
  const cacheDir = path.join(outDir, 'npm-cache');
  fs.mkdirSync(tarballDir, { recursive: true });
  fs.mkdirSync(consumerDir, { recursive: true });
  fs.mkdirSync(rawDir, { recursive: true });

  execFileSync(
    process.execPath,
    [
      path.join(root, 'npm', 'scripts', 'pack-platform.mjs'),
      '--target',
      target,
      '--binary',
      binaryPath,
      '--version',
      version,
      '--out',
      platformStaging,
    ],
    { cwd: root, stdio: 'inherit' },
  );
  execFileSync(
    process.execPath,
    [
      path.join(root, 'npm', 'scripts', 'prepare-preview-launcher.mjs'),
      '--version',
      version,
      '--out',
      launcherStaging,
    ],
    { cwd: root, stdio: 'inherit' },
  );

  const platformTarball = packTarball(platformStaging, tarballDir);
  const launcherTarball = packTarball(launcherStaging, tarballDir);

  fs.writeFileSync(
    path.join(consumerDir, 'package.json'),
    `${JSON.stringify({ name: 'vue-vet-consumer', private: true, version: '0.0.0' })}\n`,
  );

  const installArgs = [
    'install',
    '--omit=optional',
    '--offline',
    '--no-audit',
    '--no-fund',
    '--ignore-scripts',
    '--loglevel=error',
    platformTarball.path,
    launcherTarball.path,
  ];
  const install = spawnNpm(installArgs, {
    cwd: consumerDir,
    env: {
      ...process.env,
      npm_config_cache: cacheDir,
      npm_config_update_notifier: 'false',
    },
  });
  writeRaw(rawDir, 'install', install);

  /** @type {{ name: string, ok: boolean, expected: unknown, actual: unknown }[]} */
  const checks = [];

  /**
   * @param {string} name
   * @param {unknown} expected
   * @param {unknown} actual
   * @param {boolean} [ok]
   */
  function record(name, expected, actual, ok = Object.is(expected, actual)) {
    checks.push({ name, ok, expected, actual });
  }

  record('install-exit', 0, install.status);

  const launcherInstalled = path.join(consumerDir, 'node_modules', '@vue-vet', 'cli');
  const launcherPkgFile = path.join(launcherInstalled, 'package.json');
  const launcherBinJs = path.join(launcherInstalled, 'bin', 'vue-vet.js');
  /** @type {Record<string, unknown> | null} */
  let launcherPkg = null;
  if (fs.existsSync(launcherPkgFile)) {
    launcherPkg = /** @type {Record<string, unknown>} */ (readJson(launcherPkgFile));
  }
  record('launcher-package-version', version, launcherPkg?.version ?? null);
  const launcherBinField =
    launcherPkg && typeof launcherPkg.bin === 'object' && launcherPkg.bin !== null
      ? /** @type {Record<string, unknown>} */ (launcherPkg.bin)['vue-vet']
      : null;
  record('launcher-bin', 'bin/vue-vet.js', launcherBinField ?? null);

  const osCpu = `${entry.os}-${entry.cpu}`;
  const platformInstalled = path.join(consumerDir, 'node_modules', '@vue-vet', osCpu);
  const platformPkgFile = path.join(platformInstalled, 'package.json');
  /** @type {Record<string, unknown> | null} */
  let platformPkg = null;
  if (fs.existsSync(platformPkgFile)) {
    platformPkg = /** @type {Record<string, unknown>} */ (readJson(platformPkgFile));
  }
  record('platform-package-version', version, platformPkg?.version ?? null);
  record(
    'platform-os',
    [entry.os],
    platformPkg?.os ?? null,
    JSON.stringify(platformPkg?.os) === JSON.stringify([entry.os]),
  );
  record(
    'platform-cpu',
    [entry.cpu],
    platformPkg?.cpu ?? null,
    JSON.stringify(platformPkg?.cpu) === JSON.stringify([entry.cpu]),
  );

  const installedBinary = path.join(platformInstalled, 'bin', entry.bin);
  if (!fs.existsSync(installedBinary)) {
    record('installed-binary-sha256', binarySha, null, false);
    record('installed-binary-size', binarySize, null, false);
  } else {
    record('installed-binary-sha256', binarySha, sha256File(installedBinary));
    record('installed-binary-size', binarySize, fs.statSync(installedBinary).size);
    if (process.platform !== 'win32') {
      const mode = fs.statSync(installedBinary).mode;
      record('installed-binary-executable', true, (mode & 0o111) !== 0);
    }
  }

  if (process.platform === 'win32') {
    const cmdPath = path.join(consumerDir, 'node_modules', '.bin', 'vue-vet.cmd');
    if (!fs.existsSync(cmdPath)) {
      record('bin-vue-vet-ownership', '@vue-vet/cli/bin/vue-vet.js', null, false);
    } else {
      const text = fs.readFileSync(cmdPath, 'utf8');
      const ok =
        text.includes('@vue-vet\\cli\\bin\\vue-vet.js') ||
        text.includes('@vue-vet/cli/bin/vue-vet.js');
      record('bin-vue-vet-ownership', '@vue-vet/cli/bin/vue-vet.js', text, ok);
    }
  } else {
    const shim = path.join(consumerDir, 'node_modules', '.bin', 'vue-vet');
    if (!fs.existsSync(shim)) {
      record('bin-vue-vet-ownership', launcherBinJs, null, false);
    } else {
      const isSymlink = fs.lstatSync(shim).isSymbolicLink();
      const actual = isSymlink ? fs.realpathSync(shim) : `not a symlink: ${shim}`;
      const expected = fs.existsSync(launcherBinJs) ? fs.realpathSync(launcherBinJs) : launcherBinJs;
      record('bin-vue-vet-ownership', expected, actual, isSymlink && actual === expected);
    }
  }

  const resolveJs = path.join(launcherInstalled, 'lib', 'resolve.js');
  if (!fs.existsSync(resolveJs) || !fs.existsSync(installedBinary)) {
    record('resolve-binary', installedBinary, null, false);
  } else {
    try {
      const require = createRequire(resolveJs);
      const { resolveBinary } = require(resolveJs);
      const resolved = resolveBinary({ requireFrom: launcherBinJs });
      const expected = fs.realpathSync(installedBinary);
      const actual = fs.realpathSync(resolved);
      record('resolve-binary', expected, actual);
    } catch (error) {
      record(
        'resolve-binary',
        installedBinary,
        error instanceof Error ? error.message : String(error),
        false,
      );
    }
  }

  const directVersion = captureRun(binaryPath, ['--version']);
  const installedVersion = captureRun(process.execPath, [launcherBinJs, '--version']);
  writeRaw(rawDir, 'direct-version', directVersion);
  writeRaw(rawDir, 'installed-version', installedVersion);
  const versionCmp = compareRuns(directVersion, installedVersion);
  record(
    'version-outputs',
    { stdout: directVersion.stdout, stderr: directVersion.stderr, status: directVersion.status },
    {
      stdout: installedVersion.stdout,
      stderr: installedVersion.stderr,
      status: installedVersion.status,
    },
    versionCmp.ok,
  );
  record(
    'version-contains',
    version,
    directVersion.stdout,
    directVersion.stdout.includes(version) && installedVersion.stdout.includes(version),
  );

  const directList = captureRun(binaryPath, ['--list-rules', '--format', 'json']);
  const installedList = captureRun(process.execPath, [
    launcherBinJs,
    '--list-rules',
    '--format',
    'json',
  ]);
  writeRaw(rawDir, 'direct-list-rules', directList);
  writeRaw(rawDir, 'installed-list-rules', installedList);
  const listCmp = compareRuns(directList, installedList);
  record(
    'list-rules-outputs',
    { stdout: directList.stdout, stderr: directList.stderr, status: directList.status },
    { stdout: installedList.stdout, stderr: installedList.stderr, status: installedList.status },
    listCmp.ok,
  );

  try {
    const listDoc = parseJsonText(directList.stdout);
    if (listDoc === null || typeof listDoc !== 'object' || Array.isArray(listDoc)) {
      record('list-rules-json', 'object with rules[]', typeof listDoc, false);
    } else {
      const listRecord = /** @type {Record<string, unknown>} */ (listDoc);
      if (!Array.isArray(listRecord.rules)) {
        record('list-rules-json', 'rules array', Object.keys(listRecord), false);
      } else {
        record('list-rules-json', 'rules array', 'rules array');
        const ids = new Set();
        for (const row of listRecord.rules) {
          if (row !== null && typeof row === 'object' && typeof row.id === 'string') {
            ids.add(row.id);
          }
        }
        record('list-rules-distinct-ids', ids.size, ids.size, ids.size > 0);
      }
    }
  } catch (error) {
    record(
      'list-rules-json',
      'JSON object with rules[]',
      error instanceof Error ? error.message : String(error),
      false,
    );
  }

  const scanJsonArgs = ['--format', 'json', '--no-cache', projectPath];
  const directScanJson = captureRun(binaryPath, scanJsonArgs);
  const installedScanJson = captureRun(process.execPath, [launcherBinJs, ...scanJsonArgs]);
  writeRaw(rawDir, 'direct-scan-json', directScanJson);
  writeRaw(rawDir, 'installed-scan-json', installedScanJson);
  const scanJsonCmp = compareRuns(directScanJson, installedScanJson);
  record(
    'scan-json-outputs',
    {
      stdout: directScanJson.stdout,
      stderr: directScanJson.stderr,
      status: directScanJson.status,
    },
    {
      stdout: installedScanJson.stdout,
      stderr: installedScanJson.stderr,
      status: installedScanJson.status,
    },
    scanJsonCmp.ok,
  );

  try {
    const scanDoc = parseJsonText(directScanJson.stdout);
    const found = findDiagnosticRule(scanDoc, expectRule);
    record(
      'scan-expected-rule',
      expectRule,
      found ? (found.rule_id ?? found.rule ?? found) : null,
      found !== null,
    );
  } catch (error) {
    record(
      'scan-expected-rule',
      expectRule,
      error instanceof Error ? error.message : String(error),
      false,
    );
  }

  const scanTextArgs = ['--no-cache', projectPath];
  const directScanText = captureRun(binaryPath, scanTextArgs);
  const installedScanText = captureRun(process.execPath, [launcherBinJs, ...scanTextArgs]);
  writeRaw(rawDir, 'direct-scan-text', directScanText);
  writeRaw(rawDir, 'installed-scan-text', installedScanText);
  const scanTextCmp = compareRuns(directScanText, installedScanText);
  record(
    'scan-text-outputs',
    {
      stdout: directScanText.stdout,
      stderr: directScanText.stderr,
      status: directScanText.status,
    },
    {
      stdout: installedScanText.stdout,
      stderr: installedScanText.stderr,
      status: installedScanText.status,
    },
    scanTextCmp.ok,
  );

  const npmVersion = spawnNpm(['--version'], { cwd: root });
  if (npmVersion.status !== 0) {
    throw new Error(`npm --version failed: ${(npmVersion.stderr || npmVersion.stdout).trim()}`);
  }

  const ok = checks.every((item) => item.ok);
  const result = {
    schema_version: 1,
    ok,
    started_at: startedAt,
    finished_at: new Date().toISOString(),
    node_version: process.version,
    npm_version: (npmVersion.stdout ?? '').trim(),
    platform: entry,
    workspace_version: version,
    project: projectPath,
    expect_rule: expectRule,
    binary: {
      path: binaryPath,
      size: binarySize,
      sha256: binarySha,
    },
    tarballs: {
      platform: {
        filename: platformTarball.filename,
        size: platformTarball.size,
        sha256: platformTarball.sha256,
        integrity: platformTarball.integrity,
        shasum: platformTarball.shasum,
      },
      launcher: {
        filename: launcherTarball.filename,
        size: launcherTarball.size,
        sha256: launcherTarball.sha256,
        integrity: launcherTarball.integrity,
        shasum: launcherTarball.shasum,
      },
    },
    install: {
      command: [npmCmd, ...installArgs],
      exit_code: install.status,
    },
    checks,
  };
  fs.writeFileSync(path.join(outDir, 'result.json'), `${JSON.stringify(result, null, 2)}\n`);

  for (const item of checks) {
    console.log(`${item.name} ${item.ok ? 'ok' : 'FAIL'}`);
  }
  console.log(`consumer check: ${ok ? 'PASS' : 'FAIL'}`);
  return ok;
}

try {
  const ok = main();
  process.exit(ok ? 0 : 1);
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
}
