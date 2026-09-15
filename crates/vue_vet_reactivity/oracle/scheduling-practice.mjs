/**
 * Vue 3.5.40 + VueUse 13.9.0 runtime premises for scheduling-practice rules.
 *
 * Locked oracle: this package's node_modules.
 * Independent round-7 probes stay read-only under .recovery-20260907.
 * Run: `just oracle-scheduling-practice`
 */
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import path from "node:path";

const oraclePkg = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "package.json");
const requirePkg = createRequire(oraclePkg);
const vue = requirePkg("vue");
const core = await import(requirePkg.resolve("@vueuse/core"));
assert.equal(vue.version, "3.5.40", `expected Vue 3.5.40, got ${vue.version}`);
assert.equal(requirePkg("@vueuse/core/package.json").version, "13.9.0");
assert.equal(typeof core.computedAsync, "function");

const {
  effectScope,
  isReadonly,
  nextTick,
  onScopeDispose,
  ref,
  watch,
  watchSyncEffect,
} = vue;
const { computedAsync } = core;

let failed = 0;
function check(name, fn) {
  try {
    fn();
  } catch (error) {
    failed += 1;
    console.error(`FAIL ${name}: ${error.message}`);
  }
}

async function flush() {
  for (let i = 0; i < 8; i++) {
    await Promise.resolve();
    await nextTick();
  }
}

async function scheduling(mode, stopEarly = false) {
  const scope = effectScope();
  const source = ref(0);
  const sink = ref(0);
  const seen = [];
  let hits = 0;
  let stop;
  scope.run(() => {
    watchSyncEffect(() => seen.push(sink.value));
    stop = watch(source, (value) => {
      hits++;
      sink.value = value;
    }, { flush: mode });
  });
  source.value = 1;
  const intermediateRead = sink.value;
  source.value = 2;
  if (stopEarly) stop();
  await nextTick();
  const result = { intermediateRead, settled: sink.value, seen: seen.slice(), hits };
  scope.stop();
  return result;
}

const sync = await scheduling("sync");
const queued = await scheduling("pre");
check("queued-flush-reduces-actual-callback-work", () => {
  assert.deepEqual([sync.hits, queued.hits], [2, 1]);
});
check("queued-flush-preserves-after-tick-value", () => {
  assert.deepEqual([sync.settled, queued.settled], [2, 2]);
});
check("queued-flush-changes-same-tick-direct-demand", () => {
  assert.deepEqual([sync.intermediateRead, queued.intermediateRead], [1, 0]);
});
check("queued-flush-changes-downstream-sync-observer", () => {
  assert.deepEqual([sync.seen, queued.seen], [[0, 1, 2], [0, 2]]);
});
const stopSync = await scheduling("sync", true);
const stopQueued = await scheduling("pre", true);
check("stop-before-queue-flush-changes-final-value", () => {
  assert.deepEqual([stopSync.settled, stopQueued.settled], [2, 0]);
});

async function asyncMode(lazy) {
  const scope = effectScope();
  const source = ref(1);
  let evaluations = 0;
  const value = scope.run(() =>
    computedAsync(async () => {
      evaluations++;
      return source.value * 10;
    }, -1, { lazy }),
  );
  source.value = 2;
  source.value = 3;
  await flush();
  const beforeRead = evaluations;
  const first = value.value;
  await flush();
  const settled = value.value;
  const afterFirst = evaluations;
  source.value = 4;
  await flush();
  const result = {
    beforeRead,
    first,
    settled,
    readonly: isReadonly(value),
    afterFirst,
    laterEvals: evaluations,
    laterValue: value.value,
  };
  scope.stop();
  return result;
}
const eager = await asyncMode(false);
const lazy = await asyncMode(true);
check("lazy-async-saves-pre-demand-evaluations", () => {
  assert.deepEqual([eager.beforeRead, lazy.beforeRead], [2, 0]);
});
check("lazy-async-first-demand-sees-initial-state", () => {
  assert.deepEqual([eager.first, lazy.first], [30, -1]);
});
check("lazy-async-has-equal-eventually-settled-result", () => {
  assert.deepEqual([eager.settled, lazy.settled], [30, 30]);
});
check("lazy-async-changes-ref-writability", () => {
  assert.deepEqual([eager.readonly, lazy.readonly], [false, true]);
});
check("lazy-async-stays-started-after-first-demand", () => {
  assert.deepEqual([lazy.afterFirst, lazy.laterEvals, lazy.laterValue], [1, 2, 40]);
});

async function guardedAsyncDemand(lazyMode) {
  const scope = effectScope();
  const source = ref(1);
  const sink = ref(0);
  const observations = [];
  let evaluations = 0;
  const value = scope.run(() =>
    computedAsync(async () => {
      evaluations++;
      return source.value * 10;
    }, -1, { lazy: lazyMode }),
  );
  source.value = 3;
  await flush();
  const beforeDemand = evaluations;
  scope.run(() => {
    watchSyncEffect(() => observations.push(sink.value));
    watch(value, (current) => {
      if (current !== -1) sink.value = current;
    }, { immediate: true, flush: "sync" });
  });
  await flush();
  const result = { beforeDemand, observations: observations.slice() };
  scope.stop();
  return result;
}
const eagerGuarded = await guardedAsyncDemand(false);
const lazyGuarded = await guardedAsyncDemand(true);
check("lazy-loading-tolerant-demand-preserves-observed-output", () => {
  assert.deepEqual([eagerGuarded.observations, lazyGuarded.observations], [[0, 30], [0, 30]]);
});
check("lazy-loading-tolerant-demand-avoids-pre-demand-work", () => {
  assert.deepEqual([eagerGuarded.beforeDemand, lazyGuarded.beforeDemand], [2, 0]);
});
{
  const scope = effectScope();
  let result;
  scope.run(() => {
    result = computedAsync(async () => 10, 0);
  });
  await flush();
  result.value = 99;
  check("eager-computed-async-valid-manual-result-write", () => {
    assert.equal(result.value, 99);
  });
  scope.stop();
}

function ownedChild(detached, propagatePause = false) {
  const parent = effectScope();
  const source = ref(0);
  const sink = ref(0);
  let child;
  let hits = 0;
  parent.run(() => {
    child = effectScope(detached);
    child.run(() =>
      watch(source, (value) => {
        hits++;
        sink.value = value;
      }, { flush: "sync" }),
    );
    onScopeDispose(() => child.stop());
  });
  source.value = 1;
  parent.pause();
  if (propagatePause) child.pause();
  source.value = 2;
  source.value = 3;
  const during = { hits, value: sink.value };
  parent.resume();
  if (propagatePause) child.resume();
  const resumed = { hits, value: sink.value };
  parent.stop();
  source.value = 4;
  return { during, resumed, stoppedHits: hits };
}
const detached = ownedChild(true);
const attached = ownedChild(false);
const managedPause = ownedChild(true, true);
check("owned-detached-stop-management-is-correct", () => {
  assert.deepEqual([detached.stoppedHits, attached.stoppedHits], [3, 2]);
});
check("attached-child-coalesces-work-during-parent-pause", () => {
  assert.deepEqual([detached.during, attached.during], [
    { hits: 3, value: 3 },
    { hits: 1, value: 1 },
  ]);
});
check("attached-child-preserves-output-after-resume", () => {
  assert.deepEqual([detached.resumed.value, attached.resumed.value], [3, 3]);
});
check("detached-explicit-pause-management-matches-attached", () => {
  assert.deepEqual(managedPause, attached);
});

if (failed > 0) {
  process.exitCode = 1;
} else {
  console.log("scheduling-practice oracle ok");
}
