/**
 * Vue 3.5.40 lifetime / cleanup evidence (not onTrack JSON).
 *
 *   node lifetime-runs.mjs
 */
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const requireVue = createRequire(fileURLToPath(new URL("./package.json", import.meta.url)));
const vue = requireVue("vue");
const {
  effectScope,
  nextTick,
  getCurrentWatcher,
  onScopeDispose,
  onWatcherCleanup,
  ref,
  watch,
  watchEffect,
} = vue;

assert.equal(vue.version, "3.5.40", `expected Vue 3.5.40, got ${vue.version}`);

const results = [];

for (const api of ["watch", "watchEffect"]) {
  for (const registration of ["return", "onCleanup"]) {
    const source = ref(0);
    let runs = 0;
    let cleanups = 0;
    const effect = (onCleanup) => {
      source.value;
      runs += 1;
      const cleanup = () => {
        cleanups += 1;
      };
      if (registration === "onCleanup") onCleanup(cleanup);
      return cleanup;
    };
    const stop =
      api === "watch"
        ? watch(source, (_value, _old, onCleanup) => effect(onCleanup), { immediate: true })
        : watchEffect(effect);
    source.value += 1;
    await nextTick();
    stop();
    assert.equal(cleanups, registration === "return" ? 0 : 2);
    results.push({ case: "cleanup-registration", api, registration, runs, cleanups });
  }
}

{
  const source = ref(0);
  let cleanups = 0;
  const cleanup = () => {
    cleanups += 1;
  };
  const stop = watchEffect(() => {
    source.value;
    return cleanup;
  });
  source.value += 1;
  await nextTick();
  stop();
  assert.equal(cleanups, 0);
  results.push({ case: "named-identifier-return", cleanups });
}

{
  const source = ref(0);
  let cleanups = 0;
  function effect() {
    source.value;
    return () => {
      cleanups += 1;
    };
  }
  const stop = watchEffect(effect);
  source.value += 1;
  await nextTick();
  stop();
  assert.equal(cleanups, 0);
  results.push({ case: "named-function-callback-return", cleanups });
}

{
  const source = ref(0);
  let cleanups = 0;
  let warnings = 0;
  const warn = console.warn;
  console.warn = () => {
    warnings += 1;
  };
  try {
    const stop = watchEffect(async () => {
      source.value;
      await Promise.resolve();
      onWatcherCleanup(() => {
        cleanups += 1;
      });
    });
    await nextTick();
    source.value += 1;
    await nextTick();
    stop();
    assert.equal(cleanups, 0);
    assert.equal(warnings, 2);
    results.push({ case: "onWatcherCleanup-after-await", cleanups, warnings });
  } finally {
    console.warn = warn;
  }
}

{
  const source = ref(0);
  let cleanups = 0;
  const stop = watchEffect(async (onCleanup) => {
    source.value;
    await Promise.resolve();
    onCleanup(() => {
      cleanups += 1;
    });
  });
  await nextTick();
  source.value += 1;
  await nextTick();
  stop();
  assert.equal(cleanups, 2);
  results.push({ case: "onCleanup-after-await-valid", cleanups });
}

{
  const source = ref(0);
  const scope = effectScope();
  let runs = 0;
  await scope.run(async () => {
    await Promise.resolve();
    watchEffect(() => {
      source.value;
      runs += 1;
    });
  });
  scope.stop();
  source.value += 1;
  await nextTick();
  assert.equal(runs, 2);
  results.push({ case: "orphan-after-await-ignored-handle", runsAfterStop: runs });
}

{
  const source = ref(0);
  const scope = effectScope();
  let runs = 0;
  await scope.run(async () => {
    await Promise.resolve();
    scope.run(() => {
      watchEffect(() => {
        source.value;
        runs += 1;
      });
    });
  });
  scope.stop();
  source.value += 1;
  await nextTick();
  assert.equal(runs, 1);
  results.push({ case: "sync-reentry-after-await", runsAfterStop: runs });
}

{
  let disposed = 0;
  let warnings = 0;
  const warn = console.warn;
  console.warn = () => {
    warnings += 1;
  };
  try {
    const scope = effectScope();
    await scope.run(async () => {
      await Promise.resolve();
      onScopeDispose(() => {
        disposed += 1;
      });
    });
    scope.stop();
    assert.equal(disposed, 0);
    assert.equal(warnings, 1);
    results.push({ case: "onScopeDispose-after-await", disposed, warnings });
  } finally {
    console.warn = warn;
  }
}

{
  let disposed = 0;
  let warnings = 0;
  const warn = console.warn;
  console.warn = () => {
    warnings += 1;
  };
  try {
    const scope = effectScope();
    await scope.run(async () => {
      await Promise.resolve();
      onScopeDispose(() => {
        disposed += 1;
      }, true);
    });
    scope.stop();
    assert.equal(disposed, 0);
    assert.equal(warnings, 0);
    results.push({ case: "onScopeDispose-after-await-failSilently", disposed, warnings });
  } finally {
    console.warn = warn;
  }
}

{
  let disposed = 0;
  const scope = effectScope();
  await scope.run(async () => {
    onScopeDispose(() => {
      disposed += 1;
    });
    await Promise.resolve();
  });
  scope.stop();
  assert.equal(disposed, 1);
  results.push({ case: "onScopeDispose-before-await", disposed });
}

{
  const { getCurrentWatcher } = vue;
  const source = ref(0);
  let cleanups = 0;
  let warnings = 0;
  const warn = console.warn;
  console.warn = () => {
    warnings += 1;
  };
  try {
    const stop = watchEffect(async () => {
      source.value;
      const owner = getCurrentWatcher();
      await Promise.resolve();
      onWatcherCleanup(() => {
        cleanups += 1;
      }, false, owner);
    });
    await nextTick();
    source.value += 1;
    await nextTick();
    stop();
    assert.equal(warnings, 0);
    assert.ok(cleanups >= 1);
    results.push({ case: "onWatcherCleanup-explicit-owner", cleanups, warnings });
  } finally {
    console.warn = warn;
  }
}

{
  const source = ref(0);
  const scope = effectScope();
  let runs = 0;
  await scope.run(async () => {
    await Promise.resolve();
    scope.on();
    watchEffect(() => {
      source.value;
      runs += 1;
    });
    scope.off();
  });
  scope.stop();
  source.value += 1;
  await nextTick();
  results.push({ case: "scope-on-reentry-after-await", runsAfterStop: runs });
}

{
  const source = ref(0);
  let cleanups = 0;
  let runs = 0;
  function attach(onCleanup) {
    source.value;
    runs += 1;
    const dispose = () => {
      cleanups += 1;
    };
    onCleanup(dispose);
    return dispose;
  }
  const stop = watchEffect(attach);
  source.value += 1;
  await nextTick();
  stop();
  assert.equal(cleanups, 2);
  results.push({ case: "registered-and-returned", runs, cleanups });
}

{
  let getterCalls = 0;
  let handlerCalls = 0;
  const stop = watch(
    ...[],
    () => {
      getterCalls += 1;
      return () => {};
    },
    () => {
      handlerCalls += 1;
    },
    { immediate: true },
  );
  stop();
  assert.equal(getterCalls, 1);
  assert.equal(handlerCalls, 1);
  results.push({ case: "watch-spread-slots", getterCalls, handlerCalls });
}

for (const mode of ["computed-member", "alias", "helper-run"]) {
  const scope = effectScope();
  if (mode === "computed-member") {
    scope["run"] = () => undefined;
  } else if (mode === "alias") {
    const alias = scope;
    alias.run = () => undefined;
  } else {
    const helper = {
      run(owner) {
        owner.run = () => undefined;
      },
    };
    helper.run(scope);
  }
  let calls = 0;
  scope.run(async () => {
    await Promise.resolve();
    calls += 1;
    watchEffect(() => {});
  });
  await Promise.resolve();
  assert.equal(calls, 0);
  results.push({ case: `scope-run-invalidated-${mode}`, calls });
  scope.stop();
}

{
  let cleanups = 0;
  const stop = watchEffect(async (onCleanup) => {
    await Promise.resolve();
    const dispose = () => {
      cleanups += 1;
    };
    onCleanup(dispose);
    return dispose;
  });
  await Promise.resolve();
  stop();
  assert.equal(cleanups, 1);
  results.push({ case: "after-await-bound-cleanup", cleanups });
}

{
  let cleanups = 0;
  const stop = watchEffect((onCleanup) => {
    const dispose = () => {
      cleanups += 1;
    };
    onCleanup(...[dispose]);
    return dispose;
  });
  stop();
  assert.equal(cleanups, 1);
  results.push({ case: "spread-tuple-registration", cleanups });
}

{
  let cleanups = 0;
  const source = ref({ x: 0 });
  const stop = watch(
    source,
    ({ x }, old, onCleanup) => {
      const dispose = () => {
        cleanups += 1;
      };
      onCleanup(dispose);
      return dispose;
    },
    { immediate: true },
  );
  stop();
  assert.equal(cleanups, 1);
  results.push({ case: "destructured-watch-cleanup", cleanups });
}

{
  let warns = 0;
  let cleanups = 0;
  const warn = console.warn;
  console.warn = () => {
    warns += 1;
  };
  try {
    const stop = watchEffect(async () => {
      await Promise.resolve();
      onWatcherCleanup(
        () => {
          cleanups += 1;
        },
        false,
        getCurrentWatcher(),
      );
    });
    await Promise.resolve();
    stop();
    assert.equal(cleanups, 0);
    assert.equal(warns, 1);
    results.push({ case: "owner-read-after-await", cleanups, warnings: warns });
  } finally {
    console.warn = warn;
  }
}

{
  let cleanups = 0;
  const stop = watchEffect(async () => {
    const owner = getCurrentWatcher();
    await Promise.resolve();
    onWatcherCleanup(() => {
      cleanups += 1;
    }, false, owner);
  });
  await Promise.resolve();
  stop();
  assert.equal(cleanups, 1);
  results.push({ case: "owner-captured-before-await", cleanups });
}

for (const mode of ["destructure-target", "conditional-alias"]) {
  const scope = effectScope();
  if (mode === "destructure-target") {
    ({ run: scope.run } = { run: () => undefined });
  } else {
    const alias = true ? scope : effectScope();
    alias.run = () => undefined;
  }
  let calls = 0;
  scope.run(async () => {
    await Promise.resolve();
    calls += 1;
    watchEffect(() => {});
  });
  await Promise.resolve();
  assert.equal(calls, 0);
  results.push({ case: `scope-run-invalidated-${mode}`, calls });
  scope.stop();
}

console.log(JSON.stringify({ vue: vue.version, results }, null, 2));
