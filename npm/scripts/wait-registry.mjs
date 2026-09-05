#!/usr/bin/env node
/**
 * Poll the public npm registry until packages at VERSION have a version
 * document and a fetchable tarball. Fail closed. No silent fallback.
 *
 *   node npm/scripts/wait-registry.mjs --version 0.1.22 --mode platforms
 *   node npm/scripts/wait-registry.mjs --version 0.1.22 --mode launcher --host
 */
import { createRequire } from 'node:module';
import { setTimeout as delay } from 'node:timers/promises';

const { PLATFORMS, resolvePlatform } = createRequire(import.meta.url)(
  '../vue-vet/lib/platforms.js',
);
const REGISTRY = (process.env.npm_config_registry || 'https://registry.npmjs.org').replace(
  /\/$/,
  '',
);
const REQUEST_MS = 10_000;
const POLL_MS = 5_000;
const DEADLINE_MS = 10 * 60 * 1000;
const PLATFORM_PACKAGES = PLATFORMS.map((entry) => entry.package);

function parseArgs(argv) {
  let version;
  let mode;
  let host = false;
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--version') {
      version = argv[++i];
    } else if (arg === '--mode') {
      mode = argv[++i];
    } else if (arg === '--host') {
      host = true;
    } else {
      throw new Error(`Unexpected argument: ${arg}`);
    }
  }
  if (!version || !/^\d+\.\d+\.\d+([.-].*)?$/.test(version)) {
    throw new Error('Required: --version <semver>');
  }
  if (mode !== 'platforms' && mode !== 'launcher') {
    throw new Error('Required: --mode platforms|launcher');
  }
  return { version, mode, host };
}

function metadataUrl(name, version) {
  return `${REGISTRY}/${encodeURIComponent(name)}/${version}`;
}

async function getJson(url) {
  const response = await fetch(url, {
    signal: AbortSignal.timeout(REQUEST_MS),
    headers: { accept: 'application/json' },
  });
  if (!response.ok) {
    throw new Error(`GET ${url} -> ${response.status}`);
  }
  return response.json();
}

async function tarballOk(url) {
  const head = await fetch(url, { method: 'HEAD', signal: AbortSignal.timeout(REQUEST_MS) });
  if (head.ok) {
    return;
  }
  const get = await fetch(url, { method: 'GET', signal: AbortSignal.timeout(REQUEST_MS) });
  if (get.body) {
    await get.body.cancel();
  }
  if (!get.ok) {
    throw new Error(`tarball ${url} -> ${get.status}`);
  }
}

function assertVersionDoc(name, version, body) {
  if (body.name !== name) {
    throw new Error(`${name}: manifest name=${body.name}`);
  }
  if (body.version !== version) {
    throw new Error(`${name}: manifest version=${body.version}`);
  }
  if (typeof body.dist?.tarball !== 'string' || body.dist.tarball.length === 0) {
    throw new Error(`${name}: missing dist.tarball`);
  }
}

function assertLauncher(version, body) {
  assertVersionDoc('@vue-vet/cli', version, body);
  if (body.bin?.['vue-vet'] !== 'bin/vue-vet.js') {
    throw new Error('@vue-vet/cli: missing bin.vue-vet');
  }
  for (const pkg of PLATFORM_PACKAGES) {
    if (body.optionalDependencies?.[pkg] !== version) {
      throw new Error(
        `@vue-vet/cli: optionalDependencies ${pkg}=${body.optionalDependencies?.[pkg]}`,
      );
    }
  }
}

async function inspect(name, version, kind) {
  const body = await getJson(metadataUrl(name, version));
  if (kind === 'launcher') {
    assertLauncher(version, body);
  } else {
    assertVersionDoc(name, version, body);
  }
  await tarballOk(body.dist.tarball);
}

function wanted(mode, host) {
  if (mode === 'platforms') {
    return PLATFORM_PACKAGES.map((name) => [name, 'platform']);
  }
  const list = [['@vue-vet/cli', 'launcher']];
  if (host) {
    const entry = resolvePlatform(process.platform, process.arch);
    if (!entry) {
      throw new Error(`Unsupported host ${process.platform}-${process.arch}`);
    }
    list.push([entry.package, 'platform']);
  }
  return list;
}

async function main() {
  const { version, mode, host } = parseArgs(process.argv.slice(2));
  const pending = new Map(wanted(mode, host));
  const reasons = new Map();
  const deadline = Date.now() + DEADLINE_MS;
  console.log(`wait ${[...pending.keys()].join(' ')} @${version}`);
  while (pending.size > 0) {
    for (const [name, kind] of pending) {
      try {
        await inspect(name, version, kind);
        pending.delete(name);
        reasons.delete(name);
        console.log(`ready ${name}@${version}`);
      } catch (error) {
        reasons.set(name, error instanceof Error ? error.message : String(error));
      }
    }
    if (pending.size === 0) {
      return;
    }
    if (Date.now() >= deadline) {
      for (const [name, reason] of reasons) {
        console.error(`timeout ${name}@${version}: ${reason}`);
      }
      process.exit(1);
    }
    console.log(`pending ${[...pending.keys()].join(' ')}`);
    await delay(POLL_MS);
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
