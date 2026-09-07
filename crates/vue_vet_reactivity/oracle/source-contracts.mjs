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
  markRaw,
  nextTick,
  reactive,
  readonly,
  ref,
  shallowReactive,
  shallowReadonly,
  shallowRef,
  toRaw,
  toRef,
  toRefs,
  triggerRef,
  effectScope,
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

function cloneErrorName(value) {
  try {
    structuredClone(value);
    return null;
  } catch (error) {
    return error instanceof Error ? error.name : String(error);
  }
}

{
  assert.equal(
    cloneErrorName(reactive({ count: 1 })),
    "DataCloneError",
    "structuredClone(reactive(plain)) must throw DataCloneError",
  );
  assert.equal(
    cloneErrorName(shallowReactive({ count: 1 })),
    "DataCloneError",
    "structuredClone(shallowReactive(plain)) must throw DataCloneError",
  );
  assert.equal(
    cloneErrorName(readonly({ count: 1 })),
    "DataCloneError",
    "structuredClone(readonly(plain)) must throw DataCloneError",
  );
  assert.equal(
    cloneErrorName(shallowReadonly({ count: 1 })),
    "DataCloneError",
    "structuredClone(shallowReadonly(plain)) must throw DataCloneError",
  );
  const mutated = reactive({ count: 1 });
  mutated.count = 2;
  assert.equal(
    cloneErrorName(mutated),
    "DataCloneError",
    "later field mutation must keep Proxy identity",
  );

  assert.equal(cloneErrorName({ count: 1 }), null, "plain object clones");
  assert.equal(cloneErrorName(toRaw(reactive({ count: 1 }))), null, "toRaw of fresh reactive clones");
  assert.equal(
    cloneErrorName(reactive(markRaw({ count: 1 }))),
    null,
    "reactive(markRaw(plain)) returns raw and clones",
  );
  const frozen = Object.freeze({ count: 1 });
  assert.equal(cloneErrorName(readonly(frozen)), null, "readonly(frozen) returns raw and clones");
  assert.equal(
    cloneErrorName(shallowRef({ count: 1 }).value),
    null,
    "shallowRef payload clones",
  );

  const native = globalThis.structuredClone;
  assert.equal(
    cloneErrorName(reactive({ count: 1 })),
    "DataCloneError",
    "native structuredClone still fails on a Proxy",
  );
  try {
    const key = "structuredClone";
    globalThis[key] = (value) => value;
    const state = reactive({ count: 1 });
    assert.equal(structuredClone(state), state, "dynamic globalThis write replaces native identity");
  } finally {
    globalThis.structuredClone = native;
  }

  const loopState = reactive({ count: 1 });
  try {
    const key = "structuredClone";
    for (globalThis[key] of [(value) => value]) {}
    assert.equal(structuredClone(loopState), loopState, "for-of computed write replaces native identity");
    globalThis.structuredClone = native;
    for ({ clone: globalThis.structuredClone } of [{ clone: (value) => value }]) {}
    assert.equal(structuredClone(loopState), loopState, "for-of pattern write replaces native identity");
  } finally {
    globalThis.structuredClone = native;
  }
}

// toRef(existingRef, 'value') writeback; other keys ignored; getter key ignored.
{
  const count = ref(0);
  const same = toRef(count, "value");
  assert.equal(same, count, "toRef(ref, 'value') must return the same ref");
  same.value = 1;
  assert.equal(count.value, 1, "toRef(ref, 'value') must write through");
  const ignored = toRef(count, "n");
  assert.equal(ignored, count, "toRef(ref, 'n') must still return the same ref");
  const missing = toRef(count, undefined);
  assert.equal(missing, count, "toRef(ref, undefined) must keep the ref");
  const source = {};
  const later = toRef(source, "later");
  source.later = 2;
  assert.equal(later.value, 2, "absent object property remains a live binding");
  const getter = toRef(() => 7, "value");
  assert.equal(getter.value, 7, "function source still ignores the key");

  const state = ref({ count: 0 });
  state.__v_isRef = false;
  const property = toRef(state, "count");
  assert.notEqual(property, state, "cleared marker uses the object-key overload");
  state.count = 3;
  assert.equal(property.value, 3, "cleared marker still binds the property");

  const object = ref(1);
  delete object.__v_isRef;
  const future = toRef(object, "future");
  assert.notEqual(future, object, "deleted marker uses the object-key overload");

  const callable = () => 1;
  callable.__v_isRef = true;
  callable.value = 2;
  const tagged = toRef(callable, "value");
  assert.equal(tagged, callable, "tagged callable with key value stays the same ref");

  const pattern = ref({ count: 1 });
  pattern.count = 7;
  ({ flag: pattern.__v_isRef } = { flag: false });
  const fromPattern = toRef(pattern, "count");
  assert.notEqual(fromPattern, pattern, "pattern marker write uses the object-key overload");
  assert.equal(fromPattern.value, 7, "pattern marker write still binds the property");

  const created = ref({ count: 1 });
  created.count = 7;
  class ClearMarker {
    constructor(value) {
      delete value.__v_isRef;
    }
  }
  new ClearMarker(created);
  const fromConstructor = toRef(created, "count");
  assert.notEqual(fromConstructor, created, "constructor argument uses the object-key overload");
  assert.equal(fromConstructor.value, 7, "constructor argument still binds the property");

  const taggedReceiver = ref({ count: 1 });
  taggedReceiver.count = 7;
  taggedReceiver.clear = function () {
    delete this.__v_isRef;
  };
  taggedReceiver.clear``;
  const fromTagged = toRef(taggedReceiver, "count");
  assert.notEqual(fromTagged, taggedReceiver, "tagged-template receiver uses the object-key overload");
  assert.equal(fromTagged.value, 7, "tagged-template receiver still binds the property");
}

// effectScope(callback) is a truthy detached option; body stays dormant.
{
  let ran = 0;
  const dormant = effectScope(() => {
    ran += 1;
  });
  assert.equal(ran, 0, "constructor callback must stay dormant");
  dormant.run(() => {
    ran += 1;
  });
  assert.equal(ran, 1, "scope.run must execute the callback");
  const detached = effectScope(true);
  let detachedRuns = 0;
  detached.run(() => {
    detachedRuns += 1;
  });
  assert.equal(detachedRuns, 1, "effectScope(true) must still run callbacks");
}

console.log("source-contracts oracle: ok (Vue 3.5.40)");
