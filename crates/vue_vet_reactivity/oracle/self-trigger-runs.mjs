/**
 * Vue 3.5.40 *run-count* evidence for self-write effects.
 *
 * Separate from harness.mjs onTrack JSON. This file proves execution counts,
 * not dependency-set under-approx.
 *
 *   node self-trigger-runs.mjs
 */
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import path from "node:path";

const requireVue = createRequire(fileURLToPath(new URL("./package.json", import.meta.url)));
const vue = requireVue("vue");
const {
  computed,
  nextTick,
  ref,
  watch,
  watchEffect,
  watchPostEffect,
  watchSyncEffect,
} = vue;

assert.equal(vue.version, "3.5.40", `expected Vue 3.5.40, got ${vue.version}`);

const effects = { watchEffect, watchPostEffect, watchSyncEffect };
const modes = ["assignment", "update", "helper"];
const results = [];

for (const [name, api] of Object.entries(effects)) {
  for (const mode of modes) {
    const count = ref(0);
    let runs = 0;
    function helper() {
      count.value = count.value + 1;
    }
    const stop = api(() => {
      runs += 1;
      assert.ok(runs < 5, `${name}/${mode} unbounded`);
      if (mode === "assignment") {
        count.value = count.value + 1;
      } else if (mode === "update") {
        count.value += 1;
      } else {
        helper();
      }
    });
    try {
      await nextTick();
      assert.equal(runs, 1, `${name}/${mode} initial runs`);
      assert.equal(count.value, 1, `${name}/${mode} initial value`);
      count.value = 10;
      await nextTick();
      assert.equal(runs, 2, `${name}/${mode} after external write`);
      assert.equal(count.value, 11, `${name}/${mode} after external write value`);
      results.push({
        id: `${name}-${mode}`,
        initialRuns: 1,
        runsAfterExternalChange: 2,
      });
    } finally {
      stop();
    }
  }
}

{
  const count = ref(0);
  let runs = 0;
  const stop = watch(
    count,
    () => {
      runs += 1;
      if (runs < 3) {
        count.value += 1;
      }
    },
    { immediate: true, flush: "sync" },
  );
  try {
    assert.equal(runs, 3, "watch(immediate, flush:sync) self-write control");
    results.push({ id: "watch-self-write-control", boundedRuns: 3, value: count.value });
  } finally {
    stop();
  }
}

{
  const count = ref(0);
  let runs = 0;
  const result = computed(() => {
    runs += 1;
    assert.ok(runs < 5, "computed unbounded");
    count.value = count.value + 1;
    return count.value;
  });
  const values = [result.value, result.value];
  await nextTick();
  assert.deepEqual(values, [1, 2]);
  assert.equal(runs, 2);
  results.push({ id: "computed-self-write", values, runs, count: count.value });
}

{
  const source = ref(1);
  const events = [];
  const stopWatch = watch(
    source,
    () => {
      events.push("watch");
    },
    { immediate: true, flush: "post" },
  );
  const stopEffect = watchPostEffect(() => {
    events.push("effect");
    void source.value;
  });
  const beforeTick = [...events];
  try {
    assert.deepEqual(beforeTick, ["watch"], "immediate post watch runs before tick");
    await nextTick();
    const afterTick = [...events];
    assert.deepEqual(
      afterTick,
      ["watch", "effect"],
      "watchPostEffect first run waits until after tick",
    );
    results.push({
      id: "watch-post-vs-watchPostEffect-first-run",
      beforeTick,
      afterTick,
    });
  } finally {
    stopWatch();
    stopEffect();
  }
}

const report = {
  vue: vue.version,
  oracleRoot: path.dirname(fileURLToPath(import.meta.url)),
  kind: "run-count",
  results,
};
process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
