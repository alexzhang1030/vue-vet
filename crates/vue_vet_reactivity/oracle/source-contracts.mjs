/**
 * Vue 3.5.40 runtime premises for source-contract rules (issue #224).
 *
 * Locked oracle: this package's node_modules (Vue 3.5.40).
 * Run: `just oracle-source-contracts`
 */
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import path from "node:path";

const oraclePkg = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "package.json");
const requireVue = createRequire(oraclePkg);
const vue = requireVue("vue");
assert.equal(vue.version, "3.5.40", `expected Vue 3.5.40, got ${vue.version}`);

const {
  nextTick,
  reactive,
  readonly,
  ref,
  shallowReactive,
  shallowReadonly,
  shallowRef,
  toRefs,
  triggerRef,
  watch,
  watchEffect,
} = vue;

const warnings = [];
const origWarn = console.warn;
console.warn = (...args) => {
  warnings.push(String(args[0]));
  origWarn.apply(console, args);
};

function captureWarns(fn) {
  const start = warnings.length;
  const value = fn();
  return { value, warns: warnings.slice(start) };
}

// triggerRef on reactive is a no-op; triggerRef on shallowRef notifies.
{
  const obj = reactive({ n: 1 });
  let runs = 0;
  const stop = watchEffect(
    () => {
      void obj.n;
      runs++;
    },
    { flush: "sync" },
  );
  const before = runs;
  triggerRef(obj);
  assert.equal(runs, before, "triggerRef(reactive) must not notify");
  stop();

  const r = shallowRef({ n: 1 });
  let refRuns = 0;
  const stopRef = watchEffect(
    () => {
      void r.value.n;
      refRuns++;
    },
    { flush: "sync" },
  );
  triggerRef(r);
  assert.equal(refRuns, 2, "triggerRef(shallowRef) must notify");
  stopRef();
}

// toRefs(plain) warns; toRefs(reactive) tracks.
{
  const { warns } = captureWarns(() => toRefs({ a: 1 }));
  assert.ok(
    warns.some((text) => text.includes("toRefs() expects a reactive object")),
    `toRefs(plain) must warn, got ${warns.join(" | ")}`,
  );
  const state = reactive({ a: 1 });
  const { a } = toRefs(state);
  let runs = 0;
  const stop = watchEffect(
    () => {
      void a.value;
      runs++;
    },
    { flush: "sync" },
  );
  state.a = 2;
  assert.equal(runs, 2, "toRefs(reactive) must track");
  stop();
}

// primitive reactive/readonly family returns the input and warns.
{
  for (const [api, fn] of [
    ["reactive", reactive],
    ["readonly", readonly],
    ["shallowReactive", shallowReactive],
    ["shallowReadonly", shallowReadonly],
  ]) {
    const { value, warns } = captureWarns(() => fn(1));
    assert.equal(value, 1, `${api}(1) must return the primitive`);
    assert.ok(warns.length > 0, `${api}(1) must warn`);
  }
}

// watch(unwrapped primitive) does not track later writes.
{
  const n = ref(0);
  let snapRuns = 0;
  watch(n.value, () => {
    snapRuns++;
  });
  n.value = 2;
  await nextTick();
  assert.equal(snapRuns, 0, "watch(ref.value) must not track");

  const state = reactive({ n: 0 });
  let primRuns = 0;
  watch(state.n, () => {
    primRuns++;
  });
  state.n = 2;
  await nextTick();
  assert.equal(primRuns, 0, "watch(state.n primitive) must not track");

  let getterRuns = 0;
  watch(
    () => state.n,
    () => {
      getterRuns++;
    },
  );
  state.n = 3;
  await nextTick();
  assert.equal(getterRuns, 1, "watch(() => state.n) must track");
}

// watch(object member) sees nested mutate, not replacement.
{
  const state = reactive({ nested: { x: 1 } });
  let objRuns = 0;
  watch(state.nested, () => {
    objRuns++;
  });
  state.nested.x = 2;
  await nextTick();
  assert.equal(objRuns, 1, "nested mutate must notify");
  state.nested = { x: 9 };
  await nextTick();
  assert.equal(objRuns, 1, "replacement must not retarget");
}

function assertExtractedReceiverLoss(label, collection, method, args, check) {
  const extracted = collection[method];
  assert.throws(() => extracted(...args), TypeError, `${label} bare ${method} must throw TypeError`);
  check(extracted.call(collection, ...args), `${label} ${method}.call must keep the receiver`);
  check(extracted.apply(collection, args), `${label} ${method}.apply must keep the receiver`);
  const bound = extracted.bind(collection);
  check(bound(...args), `${label} ${method}.bind must keep the receiver`);
}

{
  const map = reactive(new Map([["a", 1]]));
  assertExtractedReceiverLoss("Map.get", map, "get", ["a"], (value, message) => {
    assert.equal(value, 1, message);
  });
  assertExtractedReceiverLoss("Map.has", map, "has", ["a"], (value, message) => {
    assert.equal(value, true, message);
  });
  assertExtractedReceiverLoss("Map.set", map, "set", ["b", 2], (value, message) => {
    assert.equal(value, map, message);
  });
  const set = reactive(new Set([1]));
  assertExtractedReceiverLoss("Set.has", set, "has", [1], (value, message) => {
    assert.equal(value, true, message);
  });
  assertExtractedReceiverLoss("Set.add", set, "add", [2], (value, message) => {
    assert.equal(value, set, message);
  });
  const array = reactive([1, 2]);
  assertExtractedReceiverLoss("Array.includes", array, "includes", [1], (value, message) => {
    assert.equal(value, true, message);
  });
  assertExtractedReceiverLoss("Array.map", array, "map", [(n) => n], (value, message) => {
    assert.deepEqual(Array.from(value), [1, 2], message);
  });
  const shallow = shallowReactive(new Map([["a", 1]]));
  assertExtractedReceiverLoss("shallow Map.get", shallow, "get", ["a"], (value, message) => {
    assert.equal(value, 1, message);
  });
}

{
  const script = `
    const { createRequire } = require("node:module");
    const requireVue = createRequire(${JSON.stringify(oraclePkg)});
    const { reactive } = requireVue("@vue/reactivity");
    const array = reactive([1, 2]);
    const extracted = array.push;
    let threw = false;
    try { extracted(3); } catch (error) { threw = error instanceof TypeError; }
    const ok = reactive([1, 2]);
    const push = ok.push;
    const length = push.call(ok, 3);
    process.stdout.write(JSON.stringify({ threw, length, okLength: ok.length }));
  `;
  const isolated = spawnSync(process.execPath, ["-e", script], {
    encoding: "utf8",
    timeout: 15_000,
  });
  assert.equal(isolated.status, 0, `isolated push oracle failed: ${isolated.stderr}`);
  const result = JSON.parse(isolated.stdout);
  assert.equal(result.threw, true, "extracted Array.push must throw TypeError in isolation");
  assert.equal(result.length, 3, "Array.push.call must keep the receiver in isolation");
  assert.equal(result.okLength, 3, "successful push.call must mutate length");
}

console.log("source-contracts oracle: ok (Vue 3.5.40)");
