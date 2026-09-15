/**
 * Vue 3.5.40 runtime premises for prefer-stable-computed-identity.
 *
 * Locked oracle: this package's node_modules (Vue 3.5.40).
 * Run: `just oracle-source-contracts` (failure propagates from this script).
 */
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import path from "node:path";

const oraclePkg = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "package.json");
const requireVue = createRequire(oraclePkg);
const vue = requireVue("vue");
assert.equal(vue.version, "3.5.40", `expected Vue 3.5.40, got ${vue.version}`);

const { computed, nextTick, ref, watch } = vue;

function sameContents(left, right) {
  return (
    left.length === right.length && left.every((value, index) => Object.is(value, right[index]))
  );
}

function stableProjection(source, project) {
  return computed((previous) => {
    const next = project(source.value);
    if (previous && sameContents(previous, next)) {
      return previous;
    }
    return next;
  });
}

{
  const items = ref([1, 2]);
  let evaluations = 0;
  const doubled = computed(() => {
    evaluations += 1;
    return items.value.map((n) => n * 2);
  });
  const first = doubled.value;
  let hits = 0;
  const stop = watch(
    doubled,
    () => {
      hits += 1;
    },
    { flush: "sync" },
  );
  items.value = [1, 2];
  const second = doubled.value;
  assert.equal(hits, 1, "fresh identity notifies the watch callback");
  assert.equal(first === second, false, "original projection allocates a new array");
  assert.deepEqual(first, [2, 4]);
  assert.deepEqual(second, [2, 4]);
  assert.equal(evaluations, 2, "getter still reruns on the equal-content replacement");
  items.value = [3, 4];
  assert.deepEqual(doubled.value, [6, 8]);
  assert.equal(hits, 2);
  stop();
}

{
  const items = ref([1, 2]);
  let evaluations = 0;
  const doubled = stableProjection(items, (value) => {
    evaluations += 1;
    return value.map((n) => n * 2);
  });
  const first = doubled.value;
  let hits = 0;
  const stop = watch(
    doubled,
    () => {
      hits += 1;
    },
    { flush: "sync" },
  );
  items.value = [1, 2];
  assert.equal(hits, 0, "reused identity suppresses the watch callback");
  assert.equal(doubled.value === first, true, "previous-value reuse keeps the array identity");
  assert.deepEqual(doubled.value, [2, 4]);
  assert.equal(evaluations, 2, "reactive reads still happen before reuse");
  items.value = [3, 4];
  assert.deepEqual(doubled.value, [6, 8]);
  assert.equal(hits, 1);
  assert.equal(evaluations, 3);
  stop();
}

{
  const items = ref([1, 2]);
  const doubled = computed(() => items.value.map((n) => n * 2));
  let childRuns = 0;
  const child = computed(() => {
    childRuns += 1;
    return doubled.value[0];
  });
  void child.value;
  items.value = [1, 2];
  const after = child.value;
  assert.equal(childRuns, 2, "downstream computed reruns on the new identity");
  assert.equal(after, 2, "downstream primitive output can stay equal");
}

{
  const items = ref([1, 2]);
  const doubled = stableProjection(items, (value) => value.map((n) => n * 2));
  let childRuns = 0;
  const child = computed(() => {
    childRuns += 1;
    return doubled.value[0];
  });
  void child.value;
  items.value = [1, 2];
  assert.equal(child.value, 2);
  assert.equal(childRuns, 1, "stable identity avoids the extra downstream computed run");
}

{
  const count = ref(2);
  const doubled = computed(() => count.value * 2);
  let hits = 0;
  watch(doubled, () => {
    hits += 1;
  }, { flush: "sync" });
  const first = doubled.value;
  count.value = 2;
  assert.equal(hits, 0, "same-value primitive source does not notify");
  assert.equal(doubled.value, first);
}

{
  const items = ref([1, 2]);
  const doubled = computed(() => items.value.map((n) => n * 2));
  let hits = 0;
  watch(doubled, () => {
    hits += 1;
  }, { flush: "sync" });
  void doubled.value;
  items.value = [1, 3];
  assert.equal(hits, 1, "changed projected contents still notify");
  assert.deepEqual(doubled.value, [2, 6]);
}

{
  const items = ref([1, 2]);
  const doubled = computed(() => items.value.map((n) => n * 2));
  items.value = [1, 2];
  let hits = 0;
  watch(doubled, () => {
    hits += 1;
  }, { flush: "sync" });
  assert.equal(hits, 0, "a watch established after replacement does not see that identity change");
}

{
  const items = ref([1, 2]);
  const doubled = computed(() => items.value.map((n) => n * 2));
  let hits = 0;
  const stop = watch(doubled, () => {
    hits += 1;
  }, { flush: "sync" });
  stop();
  items.value = [1, 2];
  await nextTick();
  assert.equal(hits, 0, "a stopped consumer does not repeat work");
}

{
  const items = ref([NaN]);
  const copy = computed(() => items.value.map((n) => n));
  let hits = 0;
  watch(copy, () => {
    hits += 1;
  }, { flush: "sync" });
  void copy.value;
  items.value = [NaN];
  assert.equal(hits, 1, "NaN contents are Object.is-equal but still a new array");
  assert.equal(Number.isNaN(copy.value[0]), true);
}

{
  const items = ref([0]);
  const copy = computed(() => items.value.map((n) => n));
  let hits = 0;
  watch(copy, () => {
    hits += 1;
  }, { flush: "sync" });
  void copy.value;
  items.value = [-0];
  assert.equal(hits, 1, "signed zero is not Object.is-equal to +0");
  assert.equal(Object.is(copy.value[0], -0), true);
}

{
  const items = ref([1, 2]);
  let dynamic = 0;
  const doubled = computed(() => {
    dynamic += 1;
    return items.value.map((n) => n * 2);
  });
  let hits = 0;
  watch(doubled, () => {
    hits += 1;
  }, { flush: "pre" });
  void doubled.value;
  items.value = [1, 2];
  await nextTick();
  assert.equal(hits, 1, "pre flush still notifies on identity change");
  assert.equal(dynamic >= 2, true);
}

{
  const items = ref([1, 2]);
  const doubled = computed(() => items.value.map((n) => n * 2));
  const escaped = doubled.value;
  escaped.push(9);
  let hits = 0;
  watch(doubled, () => {
    hits += 1;
  }, { flush: "sync" });
  items.value = [1, 2];
  assert.equal(hits, 1);
  assert.notEqual(doubled.value, escaped);
}

{
  const items = ref([1, 2]);
  const doubled = computed(() => items.value.map((n) => n * 2));
  let runs = 0;
  const first = computed(() => {
    runs += 1;
    return doubled.value[0];
  });
  const initial = first.value;
  items.value = [1, 2];
  await nextTick();
  assert.equal(initial, 2);
  assert.equal(runs, 1, "lazy child without later demand does not rerun");
}

{
  const items = ref([1, 2]);
  const doubled = computed(() => items.value.map((n) => n * 2));
  const observed = [];
  const stop = watch(doubled, (value) => observed.push(value.slice()), { flush: "pre" });
  items.value = [1, 2];
  stop();
  await nextTick();
  assert.deepEqual(observed, [], "queued pre watcher stopped after replace does not deliver");
}

{
  function run(undefined) {
    const items = ref([undefined]);
    const copy = computed(() => items.value.map((n) => n));
    const initial = copy.value.slice();
    const observed = [];
    watch(copy, (value) => observed.push(value.slice()), { flush: "sync" });
    items.value = [void 0];
    return { initial, observed };
  }
  const result = run(1);
  assert.deepEqual(result.initial, [1]);
  assert.equal(result.observed.length, 1);
  assert.equal(result.observed[0][0], undefined);
}

{
  const items = ref(["a"]);
  const selected = computed(() => items.value.filter((n) => n < "m"));
  const initial = selected.value.slice();
  const observed = [];
  watch(selected, (value) => observed.push(value.slice()), { flush: "sync" });
  items.value = ["b"];
  assert.deepEqual(initial, ["a"]);
  assert.deepEqual(observed, [["b"]], "string relational filter changes contents");
}

{
  const items = ref([1]);
  items.value = [2];
  const doubled = computed(() => items.value.map((n) => n * 2));
  const initial = doubled.value.slice();
  const observed = [];
  watch(doubled, (value) => observed.push(value.slice()), { flush: "sync" });
  items.value = [1];
  assert.deepEqual(initial, [4]);
  assert.deepEqual(observed, [[2]], "baseline is the populated source, not the declaration");
}

console.log("computed-identity oracle: ok");
