/**
 * Vue 3.5.40 late cancellation-guard / stale-settlement evidence (not onTrack JSON).
 *
 *   node stale-settlement-runs.mjs
 */
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const requireVue = createRequire(fileURLToPath(new URL("./package.json", import.meta.url)));
const vue = requireVue("vue");
const { nextTick, onWatcherCleanup, ref, watch, watchEffect } = vue;

assert.equal(vue.version, "3.5.40", `expected Vue 3.5.40, got ${vue.version}`);

function deferred() {
  let resolve;
  const promise = new Promise((next) => {
    resolve = next;
  });
  return { promise, resolve };
}

async function reversedWrites(registerAfterAwait, { flush = "sync", once = false } = {}) {
  const pending = { 1: deferred(), 2: deferred() };
  const id = ref(1);
  const out = ref(null);
  const writes = [];
  watch(
    id,
    async (value, _previous, onCleanup) => {
      if (!registerAfterAwait) {
        let cancelled = false;
        onCleanup(() => {
          cancelled = true;
        });
        const data = await pending[value].promise;
        if (!cancelled) {
          writes.push(data);
          out.value = data;
        }
        return;
      }
      const data = await pending[value].promise;
      let cancelled = false;
      onCleanup(() => {
        cancelled = true;
      });
      if (!cancelled) {
        writes.push(data);
        out.value = data;
      }
    },
    { flush, immediate: true, once },
  );
  id.value = 2;
  pending[2].resolve("two");
  await pending[2].promise;
  await Promise.resolve();
  const afterFast = out.value;
  pending[1].resolve("one");
  await pending[1].promise;
  await Promise.resolve();
  return { afterFast, result: out.value, writes: [...writes] };
}

const late = await reversedWrites(true);
assert.equal(late.afterFast, "two");
assert.equal(late.result, "one");
assert.deepEqual(late.writes, ["two", "one"]);

const latePre = await reversedWrites(true, { flush: "pre" });
assert.equal(latePre.result, "one");

const syncGuard = await reversedWrites(false);
assert.equal(syncGuard.result, "two");
assert.deepEqual(syncGuard.writes, ["two"]);

{
  const pending = { 1: deferred(), 2: deferred() };
  const id = ref(1);
  const out = ref(null);
  const writes = [];
  let generation = 0;
  watch(
    id,
    async (value) => {
      const current = (generation += 1);
      const data = await pending[value].promise;
      if (current === generation) {
        writes.push(data);
        out.value = data;
      }
    },
    { flush: "sync", immediate: true },
  );
  id.value = 2;
  pending[2].resolve("two");
  await pending[2].promise;
  await Promise.resolve();
  pending[1].resolve("one");
  await pending[1].promise;
  await Promise.resolve();
  assert.equal(out.value, "two");
  assert.deepEqual(writes, ["two"]);
}

{
  const pending = { 1: deferred(), 2: deferred() };
  const id = ref(1);
  const out = ref(null);
  const writes = [];
  watch(
    id,
    async (value) => {
      const data = await pending[value].promise;
      if (id.value === value) {
        writes.push(data);
        out.value = data;
      }
    },
    { flush: "sync", immediate: true },
  );
  id.value = 2;
  pending[2].resolve("two");
  await pending[2].promise;
  await Promise.resolve();
  pending[1].resolve("one");
  await pending[1].promise;
  await Promise.resolve();
  assert.equal(out.value, "two");
  assert.deepEqual(writes, ["two"]);
}

{
  const pending = { 1: deferred(), 2: deferred() };
  const id = ref(1);
  const results = {};
  watch(
    id,
    async (value) => {
      results[value] = await pending[value].promise;
    },
    { flush: "sync", immediate: true },
  );
  id.value = 2;
  pending[2].resolve("two");
  await pending[2].promise;
  await Promise.resolve();
  pending[1].resolve("one");
  await pending[1].promise;
  await Promise.resolve();
  assert.equal(results[1], "one");
  assert.equal(results[2], "two");
}

{
  const pending = deferred();
  const id = ref(1);
  const out = ref(null);
  const stop = watch(
    id,
    async (value, _previous, onCleanup) => {
      const data = await pending.promise;
      let cancelled = false;
      onCleanup(() => {
        cancelled = true;
      });
      if (!cancelled) out.value = data;
    },
    { flush: "sync", immediate: true },
  );
  stop();
  pending.resolve("late");
  await pending.promise;
  await Promise.resolve();
  assert.equal(out.value, "late");
}

{
  const first = deferred();
  const second = deferred();
  const id = ref(1);
  const cleanups = [];
  watch(
    id,
    async (value, _previous, onCleanup) => {
      const pending = value === 1 ? first : second;
      await pending.promise;
      onCleanup(() => {
        cleanups.push(value);
      });
    },
    { flush: "sync", immediate: true },
  );
  first.resolve("one");
  await first.promise;
  await Promise.resolve();
  id.value = 2;
  second.resolve("two");
  await second.promise;
  await Promise.resolve();
  assert.deepEqual(cleanups, [1]);
}

{
  const pending = { 1: deferred(), 2: deferred() };
  const id = ref(1);
  const out = ref(null);
  watchEffect(async (onCleanup) => {
    const value = id.value;
    const data = await pending[value].promise;
    let cancelled = false;
    onCleanup(() => {
      cancelled = true;
    });
    if (!cancelled) out.value = data;
  });
  id.value = 2;
  pending[2].resolve("two");
  await pending[2].promise;
  await Promise.resolve();
  pending[1].resolve("one");
  await pending[1].promise;
  await Promise.resolve();
  await nextTick();
  assert.equal(out.value, "one");
}

{
  const pending = deferred();
  const id = ref(1);
  const out = ref(null);
  watch(
    id,
    async (value) => {
      let cancelled = false;
      onWatcherCleanup(() => {
        cancelled = true;
      });
      const data = await pending.promise;
      if (!cancelled) out.value = data;
    },
    { flush: "sync", immediate: true },
  );
  id.value = 2;
  pending.resolve("two");
  await pending.promise;
  await Promise.resolve();
  assert.equal(out.value, "two");
}

{
  // `once` stops the watcher after the sync prefix of the first run
  // (`_cb(...args); watchHandle()` → `effect.stop()`). There is never a
  // competing run. A late guard still writes the single result; a sync
  // guard is run by stop() before the await settles, so the write is dropped.
  async function onceWrites(registerAfterAwait) {
    const pending = deferred();
    const id = ref(1);
    const out = ref(null);
    const writes = [];
    watch(
      id,
      async (value, _previous, onCleanup) => {
        if (!registerAfterAwait) {
          let cancelled = false;
          onCleanup(() => {
            cancelled = true;
          });
          const data = await pending.promise;
          if (!cancelled) {
            writes.push(data);
            out.value = data;
          }
          return;
        }
        const data = await pending.promise;
        let cancelled = false;
        onCleanup(() => {
          cancelled = true;
        });
        if (!cancelled) {
          writes.push(data);
          out.value = data;
        }
      },
      { flush: "sync", immediate: true, once: true },
    );
    pending.resolve("one");
    await pending.promise;
    await Promise.resolve();
    return { result: out.value, writes: [...writes] };
  }
  const onceLate = await onceWrites(true);
  assert.equal(onceLate.result, "one");
  assert.deepEqual(onceLate.writes, ["one"]);
  const onceSync = await onceWrites(false);
  assert.equal(onceSync.result, null);
  assert.deepEqual(onceSync.writes, []);
}

console.log(
  JSON.stringify({
    vue: vue.version,
    id: "vue-vet/reactivity/no-late-cancellation-guard",
    late,
    latePre: latePre.result,
    syncGuard,
    onceLate: { result: "one", writes: ["one"] },
    onceSync: { result: null, writes: [] },
    generation: "two",
    equality: "two",
    keyed: { 1: "one", 2: "two" },
    stopThenSettlement: "late",
    lateRegistrarLaterInvalidation: [1],
  }),
);
