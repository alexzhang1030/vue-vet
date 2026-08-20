'use strict';

const { spawn } = require('node:child_process');
const { access } = require('node:fs/promises');
const { constants } = require('node:fs');
const path = require('node:path');

/**
 * Prefer a Cargo-built binary under the workspace (debug, then release).
 * @param {string} workspaceRoot
 * @param {{ accessImpl?: typeof access }} [options]
 * @returns {Promise<string | null>}
 */
async function findWorkspaceBinary(workspaceRoot, options = {}) {
  const accessImpl = options.accessImpl || access;
  if (!workspaceRoot) {
    return null;
  }
  const candidates = [
    path.join(workspaceRoot, 'target', 'debug', 'vue-vet'),
    path.join(workspaceRoot, 'target', 'release', 'vue-vet'),
  ];
  if (process.platform === 'win32') {
    candidates.push(
      path.join(workspaceRoot, 'target', 'debug', 'vue-vet.exe'),
      path.join(workspaceRoot, 'target', 'release', 'vue-vet.exe'),
    );
  }
  for (const candidate of candidates) {
    try {
      await accessImpl(candidate, constants.X_OK).catch(async () => {
        await accessImpl(candidate, constants.F_OK);
      });
      return candidate;
    } catch {
      // try next
    }
  }
  return null;
}

/**
 * Resolve how to invoke vue-vet.
 * Order: configured path → workspace target/{debug,release}/vue-vet → PATH → npx.
 * @param {string} configuredPath
 * @param {string} [workspaceRoot]
 * @param {{
 *   accessImpl?: typeof access,
 *   commandExistsImpl?: (name: string) => Promise<boolean>,
 * }} [options]
 * @returns {Promise<{ command: string, argsPrefix: string[] }>}
 */
async function resolveLauncher(configuredPath, workspaceRoot = '', options = {}) {
  const accessImpl = options.accessImpl || access;
  const commandExistsImpl = options.commandExistsImpl || commandExists;
  const trimmed = (configuredPath || '').trim();
  if (trimmed) {
    await accessImpl(trimmed, constants.X_OK).catch(async () => {
      await accessImpl(trimmed, constants.F_OK);
    });
    return { command: trimmed, argsPrefix: [] };
  }

  const workspaceBinary = await findWorkspaceBinary(workspaceRoot, { accessImpl });
  if (workspaceBinary) {
    return { command: workspaceBinary, argsPrefix: [] };
  }

  if (await commandExistsImpl('vue-vet')) {
    return { command: 'vue-vet', argsPrefix: [] };
  }

  return { command: 'npx', argsPrefix: ['--yes', '@vue-vet/cli'] };
}

/**
 * @param {string} name
 */
function commandExists(name) {
  return new Promise((resolve) => {
    const probe = spawn(name, ['--version'], { stdio: 'ignore', shell: process.platform === 'win32' });
    probe.on('error', () => resolve(false));
    probe.on('close', (code) => resolve(code === 0));
  });
}

/**
 * Run vue-vet and parse the JSON report.
 * @param {{
 *   workspaceRoot: string,
 *   configuredPath?: string,
 *   extraArgs?: string[],
 *   spawnImpl?: typeof spawn,
 *   resolveLauncherImpl?: typeof resolveLauncher,
 * }} options
 */
async function runReactivityScan(options) {
  const spawnImpl = options.spawnImpl || spawn;
  const resolve = options.resolveLauncherImpl || resolveLauncher;
  const launcher = await resolve(options.configuredPath || '', options.workspaceRoot || '');
  const args = [
    ...launcher.argsPrefix,
    options.workspaceRoot,
    '--format',
    'json',
    '--print-reactivity',
    '--no-cache',
    ...(options.extraArgs || []),
  ];

  const { stdout, stderr, code } = await runProcess(spawnImpl, launcher.command, args, {
    cwd: options.workspaceRoot,
  });

  if (!stdout.trim()) {
    throw new Error(stderr.trim() || `vue-vet exited with code ${code} and produced no JSON`);
  }

  let report;
  try {
    report = JSON.parse(stdout);
  } catch (error) {
    throw new Error(
      `Failed to parse vue-vet JSON (exit ${code}): ${error instanceof Error ? error.message : error}\n${stdout.slice(0, 400)}`,
    );
  }

  if (report.ok === false && report.error) {
    throw new Error(typeof report.error === 'string' ? report.error : JSON.stringify(report.error));
  }

  return report;
}

/**
 * Run `vue-vet --explain-scope` and parse the ScopeExplain JSON (object or array).
 * @param {{
 *   workspaceRoot: string,
 *   query: string,
 *   scanPath?: string,
 *   configuredPath?: string,
 *   extraArgs?: string[],
 *   spawnImpl?: typeof spawn,
 *   resolveLauncherImpl?: typeof resolveLauncher,
 * }} options
 */
async function runExplainScope(options) {
  const spawnImpl = options.spawnImpl || spawn;
  const resolve = options.resolveLauncherImpl || resolveLauncher;
  const launcher = await resolve(options.configuredPath || '', options.workspaceRoot || '');
  const scanPath = options.scanPath || options.workspaceRoot;
  const args = [
    ...launcher.argsPrefix,
    scanPath,
    '--explain-scope',
    options.query,
    '--format',
    'json',
    '--no-cache',
    ...(options.extraArgs || []),
  ];

  const { stdout, stderr, code } = await runProcess(spawnImpl, launcher.command, args, {
    cwd: options.workspaceRoot,
  });

  if (!stdout.trim()) {
    throw new Error(stderr.trim() || `vue-vet --explain-scope exited with code ${code}`);
  }

  let payload;
  try {
    payload = JSON.parse(stdout);
  } catch (error) {
    throw new Error(
      `Failed to parse explain-scope JSON (exit ${code}): ${error instanceof Error ? error.message : error}\n${stdout.slice(0, 400)}`,
    );
  }

  if (payload && typeof payload === 'object' && payload.ok === false && payload.error) {
    throw new Error(typeof payload.error === 'string' ? payload.error : JSON.stringify(payload.error));
  }

  return payload;
}

/**
 * @param {typeof spawn} spawnImpl
 * @param {string} command
 * @param {string[]} args
 * @param {{ cwd: string }} options
 */
function runProcess(spawnImpl, command, args, options) {
  return new Promise((resolve, reject) => {
    const child = spawnImpl(command, args, {
      cwd: options.cwd,
      shell: process.platform === 'win32',
      env: process.env,
    });
    let stdout = '';
    let stderr = '';
    child.stdout?.setEncoding('utf8');
    child.stderr?.setEncoding('utf8');
    child.stdout?.on('data', (chunk) => {
      stdout += chunk;
    });
    child.stderr?.on('data', (chunk) => {
      stderr += chunk;
    });
    child.on('error', reject);
    child.on('close', (code) => resolve({ stdout, stderr, code: code ?? 1 }));
  });
}

module.exports = {
  findWorkspaceBinary,
  resolveLauncher,
  runReactivityScan,
  runExplainScope,
  commandExists,
};
