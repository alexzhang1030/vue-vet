/**
 * Vue 3.5.40 runtime premises for watch option/signature rules (issue #224).
 *
 * Locked oracle: this package's node_modules (Vue 3.5.40).
 * Run: `just oracle-watch-api` (also a step of `just oracle-source-contracts`).
 */
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import path from "node:path";

const oraclePkg = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "package.json");
const requireVue = createRequire(oraclePkg);
const vue = requireVue("vue");
assert.equal(vue.version, "3.5.40", `expected Vue 3.5.40, got ${vue.version}`);

const { nextTick, ref, watch, watchEffect } = vue;

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

{
  const n = ref(1);
  let equalsCalls = 0;
  let callbacks = 0;
  watch(
    n,
    () => {
      callbacks++;
    },
    {
      flush: "sync",
      equals: () => {
        equalsCalls++;
        return true;
      },
    },
  );
  n.value = 2;
  assert.equal(equalsCalls, 0, "equals must never run");
  assert.equal(callbacks, 1, "watch still runs the callback after a change");
}

{
  const n = ref(0);
  let runs = 0;
  const { warns } = captureWarns(() =>
    watchEffect(
      () => {
        void n.value;
        runs++;
      },
      { once: true, flush: "sync" },
    ),
  );
  n.value = 1;
  assert.equal(runs, 2, "watchEffect once does not stop reruns");
  assert.ok(
    warns.some((text) => text.includes("immediate") || text.includes("once") || text.includes("deep") || text.includes("watchEffect")),
    `watchEffect ignored keys should warn, got ${warns.join(" | ")}`,
  );
}

{
  const n = ref(0);
  let second = 0;
  watchEffect(
    () => {
      void n.value;
    },
    () => {
      second++;
    },
  );
  n.value = 1;
  await nextTick();
  assert.equal(second, 0, "watchEffect(fn, fn2) must not run fn2");
}

{
  const n = ref(0);
  let handlerRuns = 0;
  captureWarns(() =>
    watch(n, {
      handler() {
        handlerRuns++;
      },
      immediate: true,
    }),
  );
  n.value = 1;
  await nextTick();
  assert.equal(handlerRuns, 0, "{ handler } must never run");
}

{
  const n = ref(0);
  let calls = 0;
  watch(n, [
    () => {
      calls++;
    },
  ]);
  n.value = 1;
  await nextTick();
  assert.equal(calls, 1, "array callbacks are invoked once");
}

{
  const n = ref(0);
  let effectRuns = 0;
  captureWarns(() =>
    watch(n, 0, {
      flush: "sync",
    }),
  );
  const before = effectRuns;
  watchEffect(
    () => {
      void n.value;
      effectRuns++;
    },
    { flush: "sync" },
  );
  assert.ok(effectRuns >= before + 1, "control effect still runs");
  let formRuns = 0;
  captureWarns(() => {
    watch(
      () => {
        void n.value;
        formRuns++;
      },
      0,
      { flush: "sync" },
    );
  });
  n.value = 1;
  assert.equal(formRuns, 2, "falsy 0 callback uses effect-form and reruns");
}

{
  const n = ref(0);
  let runs = 0;
  captureWarns(() => {
    watch(
      () => {
        void n.value;
        runs++;
      },
      0n,
      { flush: "sync" },
    );
  });
  n.value = 1;
  assert.equal(runs, 2, "falsy 0n callback uses effect-form and reruns");
}

{
  const n = ref(0);
  let runs = 0;
  function options() {}
  options.flush = "sync";
  watchEffect(() => {
    void n.value;
    runs++;
  }, options);
  n.value = 1;
  assert.equal(runs, 2, "function-valued options with own flush:sync run twice");
}

{
  const n = ref(0);
  let runs = 0;
  watch(
    n,
    () => {
      runs++;
    },
    { flush: "sync", immediate: true },
  );
  n.value = 1;
  assert.equal(runs, 2, "valid watch callback + options still run");
}

{
  const n = ref(0);
  let runs = 0;
  watchEffect(
    () => {
      void n.value;
      runs++;
    },
    { flush: "sync", immediate: undefined },
  );
  n.value = 1;
  assert.equal(runs, 2, "undefined ignored keys stay unused and the effect reruns");
}

console.log("watch-api oracle: ok (Vue 3.5.40)");
