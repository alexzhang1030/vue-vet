/**
 * Vue 3.5.40 / VueUse 13.9.0 runtime premises for cached-result demand rules.
 *
 * Locked oracle: this package's node_modules.
 * Run: `just oracle-cached-result`
 */
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import path from "node:path";

const oraclePkg = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "package.json");
const requirePkg = createRequire(oraclePkg);
const vue = requirePkg("vue");
const core = await import(requirePkg.resolve("@vueuse/core/index.mjs"));
const shared = await import(requirePkg.resolve("@vueuse/shared/index.mjs"));
assert.equal(vue.version, "3.5.40", `expected Vue 3.5.40, got ${vue.version}`);
assert.equal(requirePkg("@vueuse/core/package.json").version, "13.9.0");
assert.equal(requirePkg("@vueuse/shared/package.json").version, "13.9.0");
assert.equal(shared.controlledComputed, shared.computedWithControl);
assert.equal(core.computedWithControl, shared.computedWithControl);
assert.equal(shared.useMemoize, undefined);

const { computed, effectScope, ref } = vue;
const { computedWithControl } = shared;
const { useMemoize } = core;

let checks = 0;
function throws(fn, name) {
  let failed = false;
  try {
    fn();
  } catch (error) {
    failed = true;
    assert.equal(error instanceof TypeError, true, `${name} must be TypeError, got ${error}`);
  }
  assert.equal(failed, true, `${name} must throw`);
  checks += 1;
}

function check(name, actual, expected) {
  assert.deepEqual(actual, expected, name);
  checks += 1;
}

{
  const source = ref(1);
  const resolve = useMemoize(() => source.value);
  resolve();
  source.value = "text";
  throws(() => resolve().toUpperCase(), "memoize cached number must fail toUpperCase");
  resolve.delete();
  check("memoize-delete-repairs-capability", resolve().toUpperCase(), "TEXT");
}

{
  const source = ref(1);
  const resolve = useMemoize(() => source.value);
  source.value = "text";
  check("memoize-first-call-after-write-uses-current", resolve().toUpperCase(), "TEXT");
}

{
  const source = ref(1);
  const resolve = useMemoize(() => source.value);
  const first = resolve();
  source.value = "text";
  check("memoize-distinct-key-uses-current", [first, resolve(1).toUpperCase()], [1, "TEXT"]);
}

{
  const revision = ref(0);
  const source = ref(1);
  const value = computedWithControl(revision, () => source.value);
  void value.value;
  source.value = "text";
  throws(() => value.value.toUpperCase(), "controlled cached number must fail toUpperCase");
  value.trigger();
  check("controlled-trigger-repairs-capability", value.value.toUpperCase(), "TEXT");
}

{
  const revision = ref(0);
  const source = ref(1);
  const value = computedWithControl([revision, source], () => source.value);
  void value.value;
  source.value = "text";
  check("controlled-listed-source-repairs-capability", value.value.toUpperCase(), "TEXT");
}

{
  const revision = ref(0);
  const source = ref(1);
  const value = computedWithControl(revision, () => source.value);
  source.value = "text";
  check("controlled-first-demand-after-write-uses-current", value.value.toUpperCase(), "TEXT");
}

{
  const source = ref(1);
  const value = computed(() => source.value);
  void value.value;
  source.value = "text";
  check("vue-computed-updates-demanded-capability", value.value.toUpperCase(), "TEXT");
}

{
  const scope = effectScope();
  const revision = ref(0);
  const source = ref(1);
  const value = scope.run(() => computedWithControl(revision, () => source.value));
  void value.value;
  scope.stop();
  source.value = "text";
  throws(
    () => value.value.toUpperCase(),
    "stopped owner still retains a failing direct demand",
  );
}

{
  const source = ref("initial");
  const resolve = useMemoize(() => source.value);
  resolve();
  source.value = 1;
  resolve();
  source.value = "latest";
  check("memoize-repeat-hit-keeps-initial", resolve().toUpperCase(), "INITIAL");
}

{
  const revision = ref(0);
  const source = ref("initial");
  const value = computedWithControl(revision, () => source.value);
  void value.value;
  source.value = 1;
  void value.value;
  source.value = "latest";
  check("controlled-repeat-hit-keeps-initial", value.value.toUpperCase(), "INITIAL");
}

{
  const original = Object.getOwnPropertyDescriptor(Number.prototype, "toUpperCase");
  try {
    Number.prototype.toUpperCase = function () {
      return "supported";
    };
    const source = ref(1);
    const resolve = useMemoize(() => source.value);
    resolve();
    source.value = "latest";
    check("memoize-direct-prototype-repair", resolve().toUpperCase(), "supported");
  } finally {
    if (original) Object.defineProperty(Number.prototype, "toUpperCase", original);
    else delete Number.prototype.toUpperCase;
  }
}

{
  const original = Object.getOwnPropertyDescriptor(Number.prototype, "toUpperCase");
  try {
    Number["prototype"]["toUpperCase"] = function () {
      return "supported";
    };
    const revision = ref(0);
    const source = ref(1);
    const value = computedWithControl(revision, () => source.value);
    void value.value;
    source.value = "latest";
    check("controlled-computed-prototype-repair", value.value.toUpperCase(), "supported");
  } finally {
    if (original) Object.defineProperty(Number.prototype, "toUpperCase", original);
    else delete Number.prototype.toUpperCase;
  }
}

assert.equal(checks, 14, `expected 14 semantic checks, got ${checks}`);
console.log(`cached-result oracle: ${checks} semantic checks passed on Vue ${vue.version} / VueUse 13.9.0`);
