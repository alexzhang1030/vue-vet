/**
 * Runtime oracle: collect Vue tracking deps via onTrack and emit JSON.
 *
 * Dep keys are normalized to { binding, key } where `binding` is the local
 * name we registered when creating the reactive source.
 *
 * Usage:
 *   node harness.mjs           # print report to stdout
 *   node harness.mjs --write   # also write expected/*.json
 */
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import {
  computed,
  effectScope,
  reactive,
  ref,
  toRaw,
  watchEffect,
} from "vue";

const root = path.dirname(fileURLToPath(import.meta.url));
const casesDir = path.join(root, "cases");
const expectedDir = path.join(root, "expected");
const write = process.argv.includes("--write");

/** @type {WeakMap<object, string>} */
const names = new WeakMap();

export function name(target, binding) {
  names.set(target, binding);
  return target;
}

export function namedRef(binding, value) {
  const r = ref(value);
  names.set(r, binding);
  // RefImpl track target is the ref object itself.
  return r;
}

export function namedReactive(binding, value) {
  const r = reactive(value);
  names.set(r, binding);
  // Reactive track targets the raw object, not the proxy.
  names.set(toRaw(r), binding);
  return r;
}

function depKey(event) {
  const binding = names.get(event.target);
  if (!binding) {
    return null;
  }
  const key =
    event.key === undefined || event.key === null
      ? null
      : typeof event.key === "symbol"
        ? event.key.toString()
        : String(event.key);
  return { binding, key };
}

function uniqueDeps(events) {
  const seen = new Set();
  const deps = [];
  for (const event of events) {
    const dep = depKey(event);
    if (!dep) continue;
    const id = `${dep.binding}\0${dep.key ?? ""}`;
    if (seen.has(id)) continue;
    seen.add(id);
    deps.push(dep);
  }
  deps.sort((a, b) =>
    a.binding === b.binding
      ? String(a.key).localeCompare(String(b.key))
      : a.binding.localeCompare(b.binding),
  );
  return deps;
}

async function loadCases() {
  const { readdir } = await import("node:fs/promises");
  const files = (await readdir(casesDir))
    .filter((name) => name.endsWith(".mjs"))
    .sort();
  const cases = [];
  for (const file of files) {
    const mod = await import(pathToFileURL(path.join(casesDir, file)).href);
    if (typeof mod.run !== "function" || typeof mod.id !== "string") {
      throw new Error(`${file} must export { id, source, run }`);
    }
    cases.push({ file, id: mod.id, source: mod.source, run: mod.run });
  }
  return cases;
}

async function main() {
  const cases = await loadCases();
  const report = [];
  for (const item of cases) {
    const events = [];
    const onTrack = (event) => {
      events.push({
        type: event.type,
        key: event.key,
        target: event.target,
      });
    };
    // Fresh registry per case (WeakMap keeps previous targets; names collide by object identity).
    await item.run({
      ref: namedRef,
      reactive: namedReactive,
      computed,
      watchEffect,
      effectScope,
      onTrack,
      name,
    });
    const runtime_deps = uniqueDeps(events);
    const record = {
      id: item.id,
      source: item.source,
      runtime_deps,
    };
    report.push(record);
    if (write) {
      await mkdir(expectedDir, { recursive: true });
      const out = path.join(expectedDir, `${item.id}.json`);
      await writeFile(out, `${JSON.stringify(record, null, 2)}\n`);
    }
  }
  console.log(JSON.stringify({ cases: report }, null, 2));
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
