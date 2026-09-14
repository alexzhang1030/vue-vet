/**
 * Vue 3.5.40 runtime premises for source-contract rules (issue #224).
 *
 * Locked oracle: this package's node_modules (Vue 3.5.40).
 * Run: `just oracle-source-contracts`
 */
import assert from "node:assert/strict";
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

console.log("source-contracts oracle: ok (Vue 3.5.40)");
