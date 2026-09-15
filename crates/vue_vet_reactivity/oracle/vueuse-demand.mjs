/**
 * Vue 3.5.40 + VueUse 13.9.0 runtime premises for VueUse demand contracts.
 *
 * Resolves Vue / VueUse from this package's lock only.
 * Run: `just oracle-vueuse-demand`
 */
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { fileURLToPath, pathToFileURL } from "node:url";
import path from "node:path";

const here = path.dirname(fileURLToPath(import.meta.url));
const oraclePkg = path.join(here, "package.json");
const require = createRequire(oraclePkg);
const vue = await import(pathToFileURL(require.resolve("vue")));
const core = await import(pathToFileURL(require.resolve("@vueuse/core")));
const shared = await import(pathToFileURL(require.resolve("@vueuse/shared")));
assert.equal(require("vue/package.json").version, "3.5.40");
assert.equal(require("@vueuse/core/package.json").version, "13.9.0");
assert.equal(require("@vueuse/shared/package.json").version, "13.9.0");

const { ref, effectScope } = vue;
const { watchIgnorable, createSharedComposable, createGlobalState } = core;

function deferred() {
  let resolve;
  const promise = new Promise((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

{
  const source = ref(0);
  const seen = [];
  const pending = deferred();
  const { ignoreUpdates, stop } = watchIgnorable(
    source,
    (value) => {
      seen.push(value);
    },
    { flush: "sync" },
  );
  try {
    const returned = ignoreUpdates(async () => {
      source.value = 1;
      await pending.promise;
      source.value = 2;
    });
    assert.equal(returned, undefined);
    assert.deepEqual(seen, []);
    pending.resolve();
    await Promise.resolve();
    assert.deepEqual(seen, [2]);
  } finally {
    stop();
  }
}

{
  const source = ref(0);
  const seen = [];
  const pending = deferred();
  const { ignoreUpdates, stop } = watchIgnorable(
    source,
    (value) => {
      seen.push(value);
    },
    { flush: "sync" },
  );
  try {
    ignoreUpdates(async () => {
      await pending.promise;
      ignoreUpdates(() => {
        source.value = 2;
      });
    });
    pending.resolve();
    await Promise.resolve();
    assert.deepEqual(seen, []);
  } finally {
    stop();
  }
}

{
  const source = ref(0);
  const seen = [];
  const { ignoreUpdates, stop } = watchIgnorable(
    source,
    (value) => {
      seen.push(value);
    },
    { flush: "sync" },
  );
  try {
    ignoreUpdates(async () => {
      await Promise.resolve();
      source.value = 0;
    });
    await Promise.resolve();
    assert.deepEqual(seen, []);
  } finally {
    stop();
  }
}

{
  const owner = effectScope();
  try {
    owner.run(() => {
      const useValue = createSharedComposable((value) => ref(value));
      const first = useValue(1);
      const second = useValue("text");
      assert.equal(first, second);
      assert.throws(() => second.value.toUpperCase(), TypeError);
    });
  } finally {
    owner.stop();
  }
}

{
  const owner = effectScope();
  try {
    owner.run(() => {
      const useValue = createSharedComposable((initial) => ref(initial));
      const first = useValue(1);
      const second = useValue(2);
      first.value = 3;
      assert.equal(second.value, 3);
    });
  } finally {
    owner.stop();
  }
}

{
  const useValue = createSharedComposable((value) => ref(value));
  const firstOwner = effectScope();
  firstOwner.run(() => useValue(1));
  firstOwner.stop();
  const secondOwner = effectScope();
  try {
    const second = secondOwner.run(() => useValue("text"));
    assert.equal(second.value.toUpperCase(), "TEXT");
  } finally {
    secondOwner.stop();
  }
}

{
  const useValue = createGlobalState((value) => ref(value));
  useValue(1);
  const second = useValue("text");
  assert.throws(() => second.value.toUpperCase(), TypeError);
}

assert.equal(typeof shared.watchIgnorable, "function");
assert.equal(typeof shared.ignorableWatch, "function");
assert.equal(typeof shared.createSharedComposable, "function");
assert.equal(typeof shared.createGlobalState, "function");

console.log("vueuse-demand oracle: ok (Vue 3.5.40, @vueuse/core 13.9.0, @vueuse/shared 13.9.0)");
