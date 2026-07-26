'use strict';

const { spawn } = require('node:child_process');
const { access } = require('node:fs/promises');
const { constants } = require('node:fs');

/**
 * Resolve how to invoke vue-vet.
 * @param {string} configuredPath
 * @returns {Promise<{ command: string, argsPrefix: string[] }>}
 */
async function resolveLauncher(configuredPath) {
  const trimmed = (configuredPath || '').trim();
  if (trimmed) {
    await access(trimmed, constants.X_OK).catch(async () => {
      await access(trimmed, constants.F_OK);
    });
    return { command: trimmed, argsPrefix: [] };
  }

  if (await commandExists('vue-vet')) {
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
 *   spawnImpl?: typeof spawn
 * }} options
 */
async function runReactivityScan(options) {
  const spawnImpl = options.spawnImpl || spawn;
  const launcher = await resolveLauncher(options.configuredPath || '');
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
  resolveLauncher,
  runReactivityScan,
  commandExists,
};
