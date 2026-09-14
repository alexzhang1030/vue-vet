/**
 * Vue 3.5.40 runtime premises for watch-callback contract rules.
 *
 * Locked oracle: this package's node_modules (Vue 3.5.40).
 * Run: `just oracle-watch-callback-contracts`
 */
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import path from "node:path";

const oraclePkg = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "package.json");
const requireVue = createRequire(oraclePkg);
const vue = requireVue("vue");
assert.equal(vue.version, "3.5.40", `expected Vue 3.5.40, got ${vue.version}`);

const { nextTick, isReactive, reactive, ref, shallowReactive, watch } = vue;

let failed = 0;
function check(name, fn) {
  try {
    fn();
  } catch (error) {
    failed += 1;
    console.error(`FAIL ${name}: ${error.message}`);
  }
}

async function checkAsync(name, fn) {
  try {
    await fn();
  } catch (error) {
    failed += 1;
    console.error(`FAIL ${name}: ${error.message}`);
  }
}

await checkAsync("once-immediate single source discards useful work", async () => {
  const n = ref(0);
  let accepted = 0;
  watch(
    n,
    (next, old) => {
      if (old === undefined) return;
      accepted += 1;
      void next;
    },
    { once: true, immediate: true, flush: "sync" },
  );
  n.value = 1;
  await nextTick();
  assert.equal(accepted, 0, "once+immediate undefined guard must skip the only run");
});

await checkAsync("tuple old === undefined still runs", async () => {
  const n = ref(0);
  let accepted = 0;
  watch(
    [n],
    (next, old) => {
      if (old === undefined) return;
      accepted += 1;
      void next;
    },
    { once: true, immediate: true, flush: "sync" },
  );
  assert.equal(accepted, 1, "tuple initial old is [] so the guard must not discard");
});

await checkAsync("reactive array is a single source", async () => {
  const list = reactive([1]);
  let accepted = 0;
  watch(
    list,
    (next, old) => {
      if (old === undefined) return;
      accepted += 1;
      void next;
    },
    { once: true, immediate: true, flush: "sync" },
  );
  assert.equal(accepted, 0, "reactive array uses the single-source undefined old value");
});

check("reactive root identity skips nested changes", () => {
  const state = reactive({ n: 1 });
  let accepted = 0;
  watch(
    state,
    (next, old) => {
      if (next === old) return;
      accepted += 1;
    },
    { flush: "sync" },
  );
  state.n = 2;
  assert.equal(accepted, 0, "nested mutate keeps the same proxy");
});

check("shallowReactive root identity skips nested changes", () => {
  const state = shallowReactive({ n: 1 });
  let accepted = 0;
  watch(
    state,
    (next, old) => {
      if (next === old) return;
      accepted += 1;
    },
    { flush: "sync" },
  );
  state.n = 2;
  assert.equal(accepted, 0, "shallowReactive nested mutate keeps the same root identity");
});

await checkAsync("deep ref identity accepts root replacement", async () => {
  const source = ref({ n: 1 });
  let accepted = 0;
  watch(
    source,
    (next, old) => {
      if (next === old) return;
      accepted += 1;
    },
    { deep: true, flush: "sync" },
  );
  source.value.n = 2;
  await nextTick();
  const nested = accepted;
  source.value = { n: 3 };
  await nextTick();
  assert.equal(nested, 0, "nested mutate on a deep ref is the same object");
  assert.equal(accepted, 1, "root replacement must pass the identity filter");
});

await checkAsync("once+immediate identity still runs the initial branch", async () => {
  const state = reactive({ n: 1 });
  let accepted = 0;
  watch(
    state,
    (next, old) => {
      if (next === old) return;
      accepted += 1;
    },
    { once: true, immediate: true, flush: "sync" },
  );
  assert.equal(accepted, 1, "initial invocation has undefined old, so identity is not equal");
});

check("void side-effect runs initial work", () => {
  const source = vue.ref(0);
  const events = [];
  watch(
    source,
    (n, old) => {
      if (old === void events.push(n)) return;
      events.push("later");
    },
    { immediate: true, once: true, flush: "sync" },
  );
  source.value += 1;
  assert.deepEqual(events, [0]);
});

check("inherited immediate is not an own option", () => {
  const state = reactive({ n: 0 });
  const initial = [];
  const stop = watch(
    state,
    (n, old) => {
      if (n === old) return;
      initial.push(n.n);
    },
    { __proto__: { immediate: true }, flush: "sync" },
  );
  state.n += 1;
  assert.deepEqual(initial, []);
  stop();
});

check("reactive(ref) identity filter still sees value changes", () => {
  const values = [];
  const proxyRef = reactive(vue.ref(0));
  const stopRef = watch(
    proxyRef,
    (n, old) => {
      if (n === old) return;
      values.push(n);
    },
    { flush: "sync" },
  );
  proxyRef.value += 1;
  assert.deepEqual(values, [1]);
  stopRef();
});

check("isRef marker on reactive object is not root identity", () => {
  const taggedValues = [];
  const taggedProxy = reactive({ __v_isRef: true, value: 0 });
  const stopTagged = watch(
    taggedProxy,
    (n, old) => {
      if (n === old) return;
      taggedValues.push(n);
    },
    { flush: "sync" },
  );
  taggedProxy.value += 1;
  assert.deepEqual(taggedValues, [1]);
  stopTagged();
});

check("post-construction isRef write is not root identity", () => {
  const taggedLater = reactive({ value: 0 });
  taggedLater.__v_isRef = true;
  const values = [];
  const stop = watch(
    taggedLater,
    (n, old) => {
      if (n === old) return;
      values.push(n);
    },
    { flush: "sync" },
  );
  taggedLater.value += 1;
  assert.deepEqual(values, [1]);
  stop();
});

check("freeze then reactive is not a proven proxy identity root", () => {
  const frozenTarget = { n: 0 };
  Object.freeze(frozenTarget);
  const frozenState = reactive(frozenTarget);
  const values = [];
  const stop = watch(
    frozenState,
    (n, old) => {
      if (n === old) return;
      values.push(n);
    },
    { flush: "sync" },
  );
  try {
    frozenState.n = 1;
  } catch {
    // frozen assignment may throw
  }
  assert.equal(values.length, 0);
  stop();
});

check("destructuring isRef marker assignment is not root identity", () => {
  const tagged = reactive({ value: 0 });
  ({ x: tagged.__v_isRef } = { x: true });
  const values = [];
  const stop = watch(
    tagged,
    (n, old) => {
      if (n === old) return;
      values.push([n, old]);
    },
    { flush: "sync" },
  );
  tagged.value += 1;
  assert.deepEqual(values, [[1, 0]]);
  stop();
});

check("spread freeze then reactive is not a proven proxy identity root", () => {
  const frozen = { n: 0 };
  Object.freeze(...[frozen]);
  const state = reactive(frozen);
  assert.equal(isReactive(state), false);
  const values = [];
  const stop = watch(
    state,
    (n, old) => {
      if (n === old) return;
      values.push(n);
    },
    { flush: "sync" },
  );
  try {
    state.n = 1;
  } catch {
    // frozen assignment may throw
  }
  assert.equal(values.length, 0);
  stop();
});

check("indexed freeze then reactive is not a proven proxy identity root", () => {
  const published = { n: 0 };
  const container = [published];
  Object.freeze(container[0]);
  const publishedState = reactive(published);
  assert.equal(isReactive(publishedState), false);
  const values = [];
  const stop = watch(
    publishedState,
    (n, old) => {
      if (n === old) return;
      values.push(n);
    },
    { flush: "sync" },
  );
  try {
    publishedState.n = 1;
  } catch {
    // frozen assignment may throw
  }
  assert.equal(values.length, 0);
  stop();
});

check("method receiver this.__v_isRef is not root identity", () => {
  const tagged = reactive({
    value: 0,
    tag: function () {
      this.__v_isRef = true;
    },
  });
  tagged.tag();
  const values = [];
  const stop = watch(
    tagged,
    (n, old) => {
      if (n === old) return;
      values.push([n, old]);
    },
    { flush: "sync" },
  );
  tagged.value += 1;
  assert.deepEqual(values, [[1, 0]]);
  stop();
});

check("tagged-template receiver this.__v_isRef is not root identity", () => {
  const tagged = reactive({
    value: 0,
    tag: function () {
      this.__v_isRef = true;
    },
  });
  tagged.tag``;
  const values = [];
  const stop = watch(
    tagged,
    (n, old) => {
      if (n === old) return;
      values.push([n, old]);
    },
    { flush: "sync" },
  );
  tagged.value += 1;
  assert.deepEqual(values, [[1, 0]]);
  stop();
});

check("sequence freeze then reactive is not a proven proxy identity root", () => {
  const frozen = { n: 0 };
  Object.freeze((0, frozen));
  const state = reactive(frozen);
  assert.equal(isReactive(state), false);
  const values = [];
  const stop = watch(
    state,
    (n, old) => {
      if (n === old) return;
      values.push(n);
    },
    { flush: "sync" },
  );
  try {
    state.n = 1;
  } catch {
    // frozen assignment may throw
  }
  assert.equal(values.length, 0);
  stop();
});

if (failed !== 0) {
  console.error(`watch-callback-contracts oracle: ${failed} failed (Vue 3.5.40)`);
  process.exit(1);
}
console.log("watch-callback-contracts oracle: ok (Vue 3.5.40, 19 checks)");
