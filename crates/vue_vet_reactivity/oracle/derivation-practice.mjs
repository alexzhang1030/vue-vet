/**
 * Vue 3.5.40 + VueUse 13.9.0 runtime premises for derivation-practice rules.
 *
 * Locked oracle: this package's node_modules.
 * Independent round-7 probes stay read-only under .recovery-20260907.
 * Run: `just oracle-derivation-practice`
 */
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import path from "node:path";

const oraclePkg = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "package.json");
const requirePkg = createRequire(oraclePkg);
const vue = requirePkg("vue");
const shared = await import(requirePkg.resolve("@vueuse/shared"));
const core = await import(requirePkg.resolve("@vueuse/core"));
assert.equal(vue.version, "3.5.40", `expected Vue 3.5.40, got ${vue.version}`);
assert.equal(requirePkg("@vueuse/core/package.json").version, "13.9.0");
assert.equal(requirePkg("@vueuse/shared/package.json").version, "13.9.0");
assert.equal(core.syncRef, shared.syncRef);

const { computed, effectScope, ref, watch, watchSyncEffect } = vue;
const { syncRef } = shared;

let failed = 0;
function check(name, fn) {
  try {
    fn();
  } catch (error) {
    failed += 1;
    console.error(`FAIL ${name}: ${error.message}`);
  }
}

function syncRefCase(direction, transform) {
  const scope = effectScope();
  const left = ref(1);
  const right = ref(0);
  let stop;
  const options = {};
  if (direction !== undefined) options.direction = direction;
  if (transform !== undefined) options.transform = transform;
  scope.run(() => {
    stop = Object.keys(options).length === 0 ? syncRef(left, right) : syncRef(left, right, options);
  });
  const createdEffects = scope.effects.length;
  const initial = [left.value, right.value];
  const seen = [];
  scope.run(() => watchSyncEffect(() => seen.push(right.value)));
  left.value = 5;
  left.value = 6;
  const result = { createdEffects, initial, final: [left.value, right.value], seen: seen.slice() };
  stop();
  scope.stop();
  return result;
}

const both = syncRefCase("both");
const ltr = syncRefCase("ltr");
check("sync-ref-one-way-removes-one-owned-watcher", () => {
  assert.deepEqual([both.createdEffects, ltr.createdEffects], [2, 1]);
});
check("sync-ref-sink-only-default-transforms-preserve-values", () => {
  assert.deepEqual([both.initial, both.final, both.seen], [ltr.initial, ltr.final, ltr.seen]);
});
const transformedBoth = syncRefCase("both", { ltr: (value) => value * 2, rtl: (value) => value + 1 });
const transformedLtr = syncRefCase("ltr", { ltr: (value) => value * 2, rtl: (value) => value + 1 });
check("sync-ref-reverse-transform-changes-source-at-initialization", () => {
  assert.deepEqual([transformedBoth.initial, transformedLtr.initial], [
    [3, 2],
    [1, 2],
  ]);
});
check("sync-ref-independent-right-write-needs-two-way-contract", () => {
  const left = ref(1);
  const right = ref(0);
  const stop = syncRef(left, right);
  right.value = 9;
  assert.equal(left.value, 9);
  stop();
});
check("sync-ref-explicit-default-options-match-omitted", () => {
  const omitted = syncRefCase();
  const scope = effectScope();
  const left = ref(1);
  const right = ref(0);
  scope.run(() =>
    syncRef(left, right, {
      direction: "both",
      flush: "sync",
      deep: false,
      immediate: true,
      transform: {},
    }),
  );
  assert.equal(scope.effects.length, omitted.createdEffects);
  left.value = 5;
  left.value = 6;
  assert.deepEqual([left.value, right.value], omitted.final);
  scope.stop();
});

function conditional(mode, initialValue = 2) {
  const scope = effectScope();
  const flag = ref(false);
  const source = ref(initialValue);
  const sink = ref(5);
  const seen = [];
  let evaluations = 0;
  const heavy = computed(() => {
    evaluations++;
    return source.value;
  });
  scope.run(() => {
    watchSyncEffect(() => seen.push(sink.value));
    if (mode === "array") {
      watch([flag, heavy], ([active, value]) => {
        if (active) sink.value = value;
      }, { flush: "sync", immediate: true });
    } else if (mode === "scalar") {
      watch(
        () => (flag.value ? heavy.value : 0),
        (value) => {
          if (flag.value) sink.value = value;
        },
        { flush: "sync", immediate: true },
      );
    } else {
      watch(
        () => [flag.value, flag.value ? heavy.value : undefined],
        ([active, value]) => {
          if (active) sink.value = value;
        },
        { flush: "sync", immediate: true },
      );
    }
  });
  return { scope, flag, source, sink, seen, evaluations: () => evaluations };
}

{
  const original = conditional("array", 0);
  const scalar = conditional("scalar", 0);
  const tuple = conditional("tuple", 0);
  original.flag.value = true;
  scalar.flag.value = true;
  tuple.flag.value = true;
  check("conditional-scalar-sentinel-collision-drops-activation", () => {
    assert.deepEqual([original.sink.value, scalar.sink.value, tuple.sink.value], [0, 5, 0]);
  });
  original.scope.stop();
  scalar.scope.stop();
  tuple.scope.stop();
}
{
  const original = conditional("array");
  const tuple = conditional("tuple");
  original.source.value = 3;
  tuple.source.value = 3;
  original.source.value = 4;
  tuple.source.value = 4;
  check("conditional-discriminator-tuple-skips-idle-evaluations", () => {
    assert.deepEqual([original.evaluations(), tuple.evaluations()], [3, 0]);
  });
  original.flag.value = true;
  tuple.flag.value = true;
  original.source.value = 6;
  tuple.source.value = 6;
  original.flag.value = false;
  tuple.flag.value = false;
  check("conditional-pure-sink-discriminator-tuple-preserves-output", () => {
    assert.deepEqual([original.seen, tuple.seen], [
      [5, 4, 6],
      [5, 4, 6],
    ]);
  });
  original.scope.stop();
  tuple.scope.stop();
}
{
  const nan = ref(Number.NaN);
  let nanHits = 0;
  watch(nan, () => {
    nanHits += 1;
  }, { flush: "sync" });
  nan.value = Number.NaN;
  check("nan-assignment-does-not-retrigger", () => {
    assert.equal(nanHits, 0);
  });
  const signed = ref(0);
  let hits = 0;
  watch(signed, () => {
    hits += 1;
  }, { flush: "sync" });
  signed.value = -0;
  check("signed-zero-is-a-changed-write", () => {
    assert.equal(hits, 1);
  });
}

check("rtl-concat-direction-is-not-ltr", () => {
  function oneWrite(direction) {
    const left = ref(1);
    const right = ref(0);
    const stop = syncRef(left, right, { direction });
    left.value = 5;
    const observed = [left.value, right.value];
    stop();
    return observed;
  }
  assert.deepEqual(oneWrite("r" + "tl"), [5, 0]);
  assert.deepEqual(oneWrite("ltr"), [5, 5]);
});
check("stopped-sync-does-not-copy-later-left-write", () => {
  const left = ref(1);
  const right = ref(0);
  const stop = syncRef(left, right);
  stop();
  left.value = 5;
  assert.equal(right.value, 1);
});
{
  const flag = ref(false);
  const source = ref(2);
  const sink = ref(5);
  const events = [];
  const heavy = computed(() => source.value);
  const stop = watch(
    [flag, heavy],
    ([active, value]) => {
      if (active) sink.value = value;
    },
    { flush: "sync", onTrigger: (event) => events.push(event.type) },
  );
  source.value = 3;
  check("watch-onTrigger-sees-idle-array-source", () => {
    assert.deepEqual(events, [undefined]);
  });
  stop();
}
{
  const flag = ref(false);
  const source = ref(2);
  const sink = ref(5);
  const oneRun = true;
  const heavy = computed(() => source.value);
  watch(
    [flag, heavy],
    ([active, value]) => {
      if (active) sink.value = value;
    },
    { flush: "sync", once: oneRun },
  );
  source.value = 3;
  flag.value = true;
  check("once-binding-consumes-budget-before-activation", () => {
    assert.equal(sink.value, 5);
  });
}
{
  const flag = ref(false);
  const source = ref(2);
  const events = [];
  const heavy = computed(() => source.value, { onTrack: (event) => events.push(event.type) });
  watch([flag, heavy], ([active, value]) => {
    if (active) void value;
  }, { flush: "sync" });
  source.value = 3;
  check("computed-onTrack-sees-idle-array-source", () => {
    assert.deepEqual(events, ["get", "get"]);
  });
}
{
  let evaluations = 0;
  const flag = ref(false);
  const source = ref(2);
  const sink = ref(5);
  const heavy = computed(() => {
    evaluations += 1;
    return source.value;
  });
  watch([flag, heavy], ([active, value]) => {
    if (active) sink.value = value;
  });
  function unused() {
    source.value = 3;
  }
  void unused;
  check("uninvoked-write-does-not-reevaluate-producer", () => {
    assert.equal(evaluations, 1);
  });
}
{
  const scope = effectScope();
  const left = ref(1);
  const right = ref(0);
  scope.run(() => {
    if (false) syncRef(left, right);
    left.value = 5;
  });
  check("skipped-sync-call-has-zero-effects-and-zero-propagation", () => {
    assert.deepEqual([scope.effects.length, right.value], [0, 0]);
  });
  scope.stop();
}
{
  const scope = effectScope();
  const flag = ref(false);
  const source = ref(2);
  const sink = ref(5);
  let evaluations = 0;
  const heavy = computed(() => {
    evaluations += 1;
    return source.value;
  });
  scope.run(() => {
    if (false) {
      watch([flag, heavy], ([active, value]) => {
        if (active) sink.value = value;
      });
    }
    source.value = 3;
  });
  check("skipped-watch-call-has-zero-producer-work", () => {
    assert.deepEqual([scope.effects.length, evaluations], [0, 0]);
  });
  scope.stop();
}
{
  const flag = ref(false);
  const source = ref(2);
  const sink = ref(5);
  let evaluations = 0;
  const heavy = computed(() => {
    evaluations += 1;
    return source.value;
  });
  const stop = watch([flag, heavy], ([active, value]) => {
    if (active) sink.value = value;
  });
  false && (source.value = 3);
  check("short-circuited-write-has-zero-reevaluations", () => {
    assert.equal(evaluations, 1);
  });
  stop();
}
{
  function activeGuard(mode) {
    const flag = ref(false);
    const source = ref(2);
    const sink = ref(5);
    let evaluations = 0;
    const heavy = computed(() => {
      evaluations += 1;
      return source.value;
    });
    function enable(target) {
      target.value = true;
    }
    enable(flag);
    const stop = watch(
      mode === "array"
        ? [flag, heavy]
        : () => [flag.value, flag.value ? heavy.value : undefined],
      ([active, value]) => {
        if (active) sink.value = value;
      },
      { flush: "sync" },
    );
    source.value = 3;
    const result = { evaluations, sink: sink.value };
    stop();
    return result;
  }
  check("escaped-guard-already-active-has-equal-work", () => {
    assert.deepEqual([activeGuard("array"), activeGuard("tuple")], [
      { evaluations: 2, sink: 3 },
      { evaluations: 2, sink: 3 },
    ]);
  });
}
{
  function aliasEdit(direction) {
    const left = ref(1);
    const right = ref(0);
    const alias = right;
    const stop = syncRef(left, right, { direction });
    left.value = 5;
    alias.value = 9;
    const result = [left.value, right.value];
    stop();
    return result;
  }
  check("template-alias-writer-needs-reverse-propagation", () => {
    assert.deepEqual([aliasEdit("both"), aliasEdit("ltr")], [
      [9, 9],
      [5, 9],
    ]);
  });
}
{
  function aliasRead(mode) {
    const flag = ref(false);
    const source = ref(2);
    const sink = ref(5);
    let evaluations = 0;
    const heavy = computed(() => {
      evaluations += 1;
      return source.value;
    });
    const shown = heavy;
    const seen = [];
    const stopReader = watchSyncEffect(() => {
      seen.push(shown.value);
    });
    const stop = watch(
      mode === "array"
        ? [flag, heavy]
        : () => [flag.value, flag.value ? heavy.value : undefined],
      ([active, value]) => {
        if (active) sink.value = value;
      },
      { flush: "sync" },
    );
    source.value = 3;
    const result = { evaluations, seen };
    stop();
    stopReader();
    return result;
  }
  check("template-alias-consumer-preserves-producer-work", () => {
    assert.deepEqual([aliasRead("array"), aliasRead("tuple")], [
      { evaluations: 2, seen: [2, 3] },
      { evaluations: 2, seen: [2, 3] },
    ]);
  });
}

if (failed > 0) {
  process.exitCode = 1;
} else {
  console.log("derivation-practice oracle ok");
}
