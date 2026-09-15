/**
 * Vue 3.5.40 / VueUse 13.9.0 runtime premises for snapshot-demand rules.
 *
 * Resolves only from this oracle package (`vue`, `@vueuse/core`, `@vueuse/shared`).
 * Run: `just oracle-snapshot-demand`
 */
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const here = path.dirname(fileURLToPath(import.meta.url));
const pkg = JSON.parse(readFileSync(path.join(here, "package.json"), "utf8"));
assert.equal(pkg.dependencies.vue, "3.5.40");
assert.equal(pkg.dependencies["@vueuse/core"], "13.9.0");
assert.equal(pkg.dependencies["@vueuse/shared"], "13.9.0");
assert.equal(existsSync(path.join(here, "node_modules/vue/package.json")), true, "missing oracle vue");
assert.equal(
  existsSync(path.join(here, "node_modules/@vueuse/core/package.json")),
  true,
  "missing oracle @vueuse/core",
);

const requirePin = createRequire(path.join(here, "package.json"));
const vue = requirePin("vue");
const vueuse = requirePin("@vueuse/core");
const vueusePkg = requirePin("@vueuse/core/package.json");
assert.equal(vue.version, "3.5.40", `expected Vue 3.5.40, got ${vue.version}`);
assert.equal(vueusePkg.version, "13.9.0", `expected VueUse 13.9.0, got ${vueusePkg.version}`);
assert.equal(vueuse.useCloned.name, "useCloned");
assert.equal(vueuse.useManualRefHistory.name, "useManualRefHistory");

const { ref, shallowRef } = vue;
const { useCloned, useManualRefHistory } = vueuse;

function throws(fn, name) {
  let failed = false;
  try {
    fn();
  } catch (error) {
    failed = true;
    assert.equal(error instanceof TypeError, true, `${name} must be TypeError, got ${error}`);
  }
  assert.equal(failed, true, `${name} must throw`);
}

{
  const { cloned } = useCloned(ref({ when: new Date("2020-01-01T00:00:00.000Z") }));
  assert.equal(typeof cloned.value.when, "string", "default JSON clone must stringify Date");
  throws(() => cloned.value.when.getTime(), "cloned Date.getTime must throw");
  throws(() => cloned.value.when.getUTCFullYear(), "cloned Date.getUTCFullYear must throw");
  assert.equal(cloned.value.when.slice(0, 4), "2020", "string consumer must succeed");
}

{
  const { cloned } = useCloned(ref({ when: new Date("2020-01-01T00:00:00.000Z") }), {
    clone: (value) => ({ when: new Date(value.when) }),
  });
  assert.equal(cloned.value.when instanceof Date, true, "custom Date clone must keep Date");
  assert.equal(cloned.value.when.getUTCFullYear(), 2020);
}

{
  const { cloned } = useCloned(ref({ when: new Date("2020-01-01T00:00:00.000Z") }));
  cloned.value.when = new Date("2021-01-01T00:00:00.000Z");
  assert.equal(cloned.value.when.getTime(), 1_609_459_200_000, "nested Date repair must restore getTime");
}

{
  const source = ref({ n: 1 });
  const { commit, undo, last, history } = useManualRefHistory(source);
  assert.equal(last.value.snapshot === source.value, true, "default history aliases source");
  commit();
  source.value.n = 2;
  assert.equal(history.value[1].snapshot.n, 2, "nested write mutates committed snapshot");
  undo();
  assert.equal(source.value.n, 2, "undo after aliased mutate restores n=2");
}

{
  const source = ref({ n: 1 });
  const { commit, undo } = useManualRefHistory(source, { clone: true });
  commit();
  source.value.n = 2;
  undo();
  assert.equal(source.value.n, 1, "clone:true undo restores n=1");
}

{
  const source = ref({ n: 1 });
  const { commit, undo } = useManualRefHistory(source, { clone: (value) => ({ n: value.n }) });
  commit();
  source.value.n = 2;
  undo();
  assert.equal(source.value.n, 1, "function clone undo restores n=1");
}

{
  const source = ref({ n: 1 });
  const { commit, undo } = useManualRefHistory(source);
  commit();
  source.value = { n: 3 };
  source.value.n = 2;
  undo();
  assert.equal(source.value.n, 1, "root replacement undo restores n=1");
}

{
  const source = ref({ n: 1 });
  source.value.n = 2;
  const { commit, undo } = useManualRefHistory(source);
  commit();
  source.value.n = 2;
  undo();
  assert.equal(source.value.n, 2, "latest-rebase undo preserves recorded n=2");
}

{
  const source = ref({ n: 1 });
  const { commit, undo } = useManualRefHistory(source);
  commit();
  source.value.n = 2;
  source.value.n = 1;
  undo();
  assert.equal(source.value.n, 1, "write restored before undo keeps n=1");
}

{
  const source = ref({ n: 1 });
  const { commit, undo } = useManualRefHistory(source);
  commit();
  undo();
  source.value.n = 2;
  undo();
  assert.equal(source.value.n, 2, "drained undo is a no-op");
}

{
  const source = ref({ n: 1 });
  const { commit, undo, clear } = useManualRefHistory(source, { capacity: 1 });
  commit();
  commit();
  assert.equal(source.value.n, 1);
  clear();
  source.value.n = 2;
  undo();
  assert.equal(source.value.n, 2, "clear then undo has no retained record");
}

{
  const source = ref({ n: 1 });
  const { reset } = useManualRefHistory(source);
  source.value.n = 2;
  reset();
  assert.equal(source.value.n, 2, "reset after nested mutate retains n=2");
}

{
  const source = shallowRef({ n: 1 });
  const { commit, undo } = useManualRefHistory(source);
  commit();
  source.value.n = 2;
  undo();
  assert.equal(source.value.n, 2, "shallowRef identity history aliases nested writes");
}

console.log(
  JSON.stringify({
    vue: vue.version,
    vueuse: vueusePkg.version,
    jsonCloneThrows: true,
    stringConsumer: "2020",
    customCloneYear: 2020,
    nestedRepair: 1_609_459_200_000,
    aliasedUndo: 2,
    cloneTrueUndo: 1,
    functionCloneUndo: 1,
    rootReplaceUndo: 1,
    latestRebase: 2,
    writeRestored: 1,
    drainedUndo: 2,
    clearThenUndo: 2,
    resetAliased: 2,
  }),
);
