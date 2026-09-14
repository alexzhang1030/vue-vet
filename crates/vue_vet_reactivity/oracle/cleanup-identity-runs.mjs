/**
 * Vue 3.5.40 EventTarget watch cleanup identity (not onTrack JSON).
 *
 *   node cleanup-identity-runs.mjs
 */
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const requireVue = createRequire(fileURLToPath(new URL("./package.json", import.meta.url)));
const vue = requireVue("vue");
const { nextTick, onWatcherCleanup, ref, shallowRef, watch } = vue;

assert.equal(vue.version, "3.5.40", `expected Vue 3.5.40, got ${vue.version}`);

function dispatch(target) {
  target.dispatchEvent(new Event("click"));
}

async function settle(flush) {
  if (flush !== "sync") {
    await nextTick();
  }
}

async function leakCase(options, register) {
  const first = new EventTarget();
  const second = new EventTarget();
  const third = new EventTarget();
  const counts = new Map([
    [first, 0],
    [second, 0],
    [third, 0],
  ]);
  const source = ref(first);
  const stop = watch(
    source,
    (target, _prev, onCleanup) => {
      const listener = () => {
        counts.set(target, (counts.get(target) ?? 0) + 1);
      };
      target.addEventListener("click", listener);
      register(onCleanup, () => {
        source.value.removeEventListener("click", listener);
      });
    },
    options,
  );
  source.value = second;
  source.value = third;
  await settle(options.flush);
  dispatch(first);
  const afterChange = counts.get(first);
  stop();
  await settle(options.flush);
  dispatch(first);
  const afterStop = counts.get(first);
  return { afterChange, afterStop };
}

async function capturedCase(options, register) {
  const first = new EventTarget();
  const second = new EventTarget();
  const third = new EventTarget();
  const counts = new Map([
    [first, 0],
    [second, 0],
    [third, 0],
  ]);
  const source = ref(first);
  const stop = watch(
    source,
    (target, _prev, onCleanup) => {
      const listener = () => {
        counts.set(target, (counts.get(target) ?? 0) + 1);
      };
      target.addEventListener("click", listener);
      register(onCleanup, () => {
        target.removeEventListener("click", listener);
      });
    },
    options,
  );
  source.value = second;
  source.value = third;
  await settle(options.flush);
  dispatch(first);
  dispatch(second);
  const afterChange = {
    first: counts.get(first),
    second: counts.get(second),
  };
  stop();
  await settle(options.flush);
  dispatch(first);
  dispatch(second);
  dispatch(third);
  const afterStop = {
    first: counts.get(first),
    second: counts.get(second),
    third: counts.get(third),
  };
  return { afterChange, afterStop };
}

const registerOnCleanup = (onCleanup, fn) => {
  onCleanup(fn);
};
const registerWatcherCleanup = (_onCleanup, fn) => {
  onWatcherCleanup(fn);
};

for (const flush of ["pre", "sync"]) {
  for (const [name, register] of [
    ["onCleanup", registerOnCleanup],
    ["onWatcherCleanup", registerWatcherCleanup],
  ]) {
    const leaked = await leakCase({ flush, immediate: true }, register);
    assert.equal(leaked.afterChange, 1, `${flush}/${name} leaked first target after change`);
    assert.equal(leaked.afterStop, 2, `${flush}/${name} leaked first target after stop`);
    const captured = await capturedCase({ flush, immediate: true }, register);
    assert.equal(captured.afterChange.first, 0, `${flush}/${name} captured first after change`);
    assert.equal(captured.afterStop.first, 0, `${flush}/${name} captured first after stop`);
    assert.equal(captured.afterStop.second, 0, `${flush}/${name} captured second after stop`);
    assert.equal(captured.afterStop.third, 0, `${flush}/${name} captured third after stop`);
  }
}

{
  const source = shallowRef(new EventTarget());
  let hits = 0;
  const handler = () => {
    hits += 1;
  };
  const first = source.value;
  const stop = watch(
    source,
    (target, _prev, onCleanup) => {
      target.addEventListener("click", handler);
      onCleanup(() => {
        source.value.removeEventListener("click", handler);
      });
    },
    { immediate: true, flush: "sync" },
  );
  source.value = new EventTarget();
  source.value = new EventTarget();
  dispatch(first);
  assert.equal(hits, 1, "shallowRef wrong cleanup still dispatches the acquired target");
  stop();
  dispatch(first);
  assert.equal(hits, 2, "shallowRef leak survives stop");
}

{
  const source = ref(new EventTarget());
  let closed = 0;
  watch(
    source,
    (target, _prev, onCleanup) => {
      void target;
      onCleanup(() => {
        void source.value;
        closed += 1;
      });
    },
    { immediate: true, flush: "sync" },
  );
  source.value = new EventTarget();
  assert.equal(closed, 1, "source read in cleanup is a valid use");
}

async function remainingHits(run) {
  let hits = 0;
  const handler = () => {
    hits += 1;
  };
  await run(handler);
  return hits;
}

{
  const hits = await remainingHits(async (handler) => {
    const source = ref(new EventTarget());
    const old = source.value;
    const stop = watch(
      source,
      (target, _prev, onCleanup) => {
        target.addEventListener("click", handler);
        onCleanup(() => {
          source.value.removeEventListener("click", handler);
        });
      },
    );
    source.value = new EventTarget();
    source.value = new EventTarget();
    await nextTick();
    const current = source.value;
    stop();
    dispatch(old);
    dispatch(current);
  });
  assert.equal(hits, 0, "default pre coalesced assignments release the acquired target");
}

{
  const hits = await remainingHits(async (handler) => {
    const source = ref(new EventTarget());
    const first = new EventTarget();
    const second = new EventTarget();
    watch(
      source,
      (target, _prev, onCleanup) => {
        target.addEventListener("click", handler);
        onCleanup(() => {
          source.value.removeEventListener("click", handler);
        });
      },
    );
    source.value = first;
    await nextTick();
    source.value = second;
    await nextTick();
    dispatch(first);
  });
  assert.equal(hits, 1, "pre schedule leaks after a proven nextTick acquisition boundary");
}

{
  const hits = await remainingHits(async (handler) => {
    const source = ref(new EventTarget());
    const old = source.value;
    watch(
      source,
      (target, _prev, onCleanup) => {
        target.addEventListener("click", handler);
        onCleanup(() => {
          source.value.removeEventListener("click", handler);
        });
      },
      { immediate: true, once: true, flush: "sync" },
    );
    source.value = new EventTarget();
    dispatch(old);
    dispatch(source.value);
  });
  assert.equal(hits, 0, "once+immediate cleanup completes before later writes");
}

{
  const hits = await remainingHits(async (handler) => {
    const source = ref(new EventTarget());
    const old = source.value;
    const stop = watch(
      source,
      (target, _prev, onCleanup) => {
        target.addEventListener("click", handler);
        onCleanup(() => {
          source.value.removeEventListener("click", handler);
        });
      },
      { immediate: true, flush: "sync" },
    );
    stop();
    source.value = new EventTarget();
    dispatch(old);
    dispatch(source.value);
  });
  assert.equal(hits, 0, "stop before replacement releases the acquired target");
}

{
  const hits = await remainingHits(async (handler) => {
    const initial = new EventTarget();
    const source = ref(initial);
    const stop = watch(
      source,
      (target, _prev, onCleanup) => {
        target.addEventListener("click", handler);
        onCleanup(() => {
          source.value.removeEventListener("click", handler);
        });
      },
      { immediate: true, flush: "sync" },
    );
    source.value = initial;
    source.value = initial;
    stop();
    dispatch(initial);
  });
  assert.equal(hits, 0, "same-allocation writeback is not a replacement");
}

{
  const hits = await remainingHits(async (handler) => {
    const source = ref(new EventTarget());
    const old = source.value;
    const stop = watch(
      source,
      (target, _prev, onCleanup) => {
        target.addEventListener("click", handler);
        onCleanup(() => {
          target.removeEventListener("click", handler);
          source.value.removeEventListener("click", handler);
        });
      },
      { immediate: true, flush: "sync" },
    );
    source.value = new EventTarget();
    stop();
    dispatch(old);
    dispatch(source.value);
  });
  assert.equal(hits, 0, "captured-target release discharges the acquisition");
}

{
  const hits = await remainingHits(async (handler) => {
    const initial = new EventTarget();
    initial.addEventListener = () => {};
    const source = ref(initial);
    const stop = watch(
      source,
      (target, _prev, onCleanup) => {
        target.addEventListener("click", handler);
        onCleanup(() => {
          source.value.removeEventListener("click", handler);
        });
      },
      { immediate: true, flush: "sync" },
    );
    source.value = new EventTarget();
    stop();
    dispatch(initial);
    dispatch(source.value);
  });
  assert.equal(hits, 0, "payload method mutation installs no native listener");
}

{
  const hits = await remainingHits(async (handler) => {
    const initial = new EventTarget();
    const source = ref(initial);
    const stop = watch(
      source,
      (target, _prev, onCleanup) => {
        target.addEventListener("click", handler);
        onCleanup(() => {
          source.value.removeEventListener("click", handler);
        });
      },
      { flush: "sync" },
    );
    source.value = initial;
    source.value = new EventTarget();
    stop();
    dispatch(initial);
    dispatch(source.value);
  });
  assert.equal(hits, 0, "same-value write before the first sync change is not an acquisition");
}

{
  const hits = await remainingHits(async (handler) => {
    const initial = new EventTarget();
    const source = ref(initial);
    const stop = watch(
      source,
      (target, _prev, onCleanup) => {
        target.addEventListener("click", handler);
        onCleanup(() => {
          source.value.removeEventListener("click", handler);
        });
      },
      { immediate: true },
    );
    source.value = new EventTarget();
    source.value = initial;
    await nextTick();
    stop();
    dispatch(initial);
  });
  assert.equal(hits, 0, "immediate pre round-trip settles back to the acquired target");
}

{
  const hits = await remainingHits(async (handler) => {
    const source = ref(new EventTarget());
    const old = source.value;
    if (false) {
      watch(
        source,
        (target, _prev, onCleanup) => {
          target.addEventListener("click", handler);
          onCleanup(() => {
            source.value.removeEventListener("click", handler);
          });
        },
        { immediate: true, flush: "sync" },
      );
    }
    source.value = new EventTarget();
    dispatch(old);
    dispatch(source.value);
  });
  assert.equal(hits, 0, "inactive watch creation installs no listener");
}

{
  const hits = await remainingHits(async (handler) => {
    const source = ref(new EventTarget());
    const old = source.value;
    const stop = watch(
      source,
      (target, _prev, onCleanup) => {
        target.addEventListener("click", handler);
        onCleanup(() => {
          source.value.removeEventListener("click", handler);
        });
      },
      { immediate: true, flush: "sync" },
    );
    const cancel = stop;
    cancel();
    source.value = new EventTarget();
    dispatch(old);
    dispatch(source.value);
  });
  assert.equal(hits, 0, "const stop-handle alias releases before replacement");
}

{
  const hits = await remainingHits(async (handler) => {
    const source = ref(new EventTarget());
    const firstAcquired = new EventTarget();
    firstAcquired.addEventListener = () => {};
    const stop = watch(
      source,
      (target, _prev, onCleanup) => {
        target.addEventListener("click", handler);
        onCleanup(() => {
          source.value.removeEventListener("click", handler);
        });
      },
      { flush: "sync" },
    );
    source.value = firstAcquired;
    source.value = new EventTarget();
    stop();
    dispatch(firstAcquired);
    dispatch(source.value);
  });
  assert.equal(hits, 0, "mutated later acquisition installs no native listener");
}

{
  const hits = await remainingHits(async () => {
    const handler = null;
    const source = ref(new EventTarget());
    const old = source.value;
    const stop = watch(
      source,
      (target, _prev, onCleanup) => {
        target.addEventListener("click", handler);
        onCleanup(() => {
          source.value.removeEventListener("click", handler);
        });
      },
      { immediate: true, flush: "sync" },
    );
    source.value = new EventTarget();
    stop();
    dispatch(old);
    dispatch(source.value);
  });
  assert.equal(hits, 0, "null listener argument creates no EventTarget listener");
}

{
  const hits = await remainingHits(async (handler) => {
    const initial = new EventTarget();
    const source = ref(initial);
    let replacement = new EventTarget();
    replacement = initial;
    const stop = watch(
      source,
      (target, _prev, onCleanup) => {
        target.addEventListener("click", handler);
        onCleanup(() => {
          source.value.removeEventListener("click", handler);
        });
      },
      { immediate: true, flush: "sync" },
    );
    source.value = replacement;
    stop();
    dispatch(initial);
    dispatch(source.value);
  });
  assert.equal(hits, 0, "mutable payload alias rebound to the acquired target is not a replacement");
}

{
  const hits = await remainingHits(async (handler) => {
    const source = ref(new EventTarget());
    const old = source.value;
    const stop = watch(
      source,
      (target, _prev, onCleanup) => {
        target.addEventListener("click", handler);
        onCleanup(() => {
          source.value.removeEventListener("click", handler);
        });
      },
      { immediate: true, flush: "sync" },
    );
    if (true) stop();
    source.value = new EventTarget();
    stop();
    dispatch(old);
    dispatch(source.value);
  });
  assert.equal(hits, 0, "earlier conditional stop releases before a later definite stop");
}

{
  const hits = await remainingHits(async (handler) => {
    const initial = new EventTarget();
    const source = ref(initial);
    let replacement = initial;
    if (false) replacement = new EventTarget();
    const stop = watch(
      source,
      (target, _prev, onCleanup) => {
        target.addEventListener("click", handler);
        onCleanup(() => {
          source.value.removeEventListener("click", handler);
        });
      },
      { immediate: true, flush: "sync" },
    );
    source.value = replacement;
    stop();
    dispatch(initial);
    dispatch(source.value);
  });
  assert.equal(hits, 0, "conditional payload write that does not execute is not a replacement");
}

{
  const hits = await remainingHits(async (handler) => {
    const initial = new EventTarget();
    const source = ref(initial);
    let replacement = initial;
    function unusedReplacement() {
      replacement = new EventTarget();
    }
    const stop = watch(
      source,
      (target, _prev, onCleanup) => {
        target.addEventListener("click", handler);
        onCleanup(() => {
          source.value.removeEventListener("click", handler);
        });
      },
      { immediate: true, flush: "sync" },
    );
    source.value = replacement;
    stop();
    dispatch(initial);
    dispatch(source.value);
  });
  assert.equal(hits, 0, "uninvoked payload write is not a replacement");
}

{
  const hits = await remainingHits(async (handler) => {
    const initial = new EventTarget();
    const source = ref(initial);
    const replacement = new EventTarget();
    const stop = watch(
      source,
      (target, _prev, onCleanup) => {
        target.addEventListener("click", handler);
        onCleanup(() => {
          source.value.removeEventListener("click", handler);
        });
      },
      { immediate: true, flush: "sync" },
    );
    source.value = replacement;
    stop();
    dispatch(initial);
    dispatch(source.value);
  });
  assert.equal(hits, 1, "stable const payload alias of a distinct allocation leaks");
}

{
  const hits = await remainingHits(async (handler) => {
    const initial = new EventTarget();
    const source = ref(initial);
    let replacement = new EventTarget();
    replacement &&= initial;
    const stop = watch(
      source,
      (target, _prev, onCleanup) => {
        target.addEventListener("click", handler);
        onCleanup(() => {
          source.value.removeEventListener("click", handler);
        });
      },
      { immediate: true, flush: "sync" },
    );
    source.value = replacement;
    stop();
    dispatch(initial);
    dispatch(source.value);
  });
  assert.equal(hits, 0, "logical payload write that preserves the acquired target is not a replacement");
}

{
  const hits = await remainingHits(async (handler) => {
    const initial = new EventTarget();
    const source = ref(initial);
    let replacement = new EventTarget();
    function run() {
      const stop = watch(
        source,
        (target, _prev, onCleanup) => {
          target.addEventListener("click", handler);
          onCleanup(() => {
            source.value.removeEventListener("click", handler);
          });
        },
        { immediate: true, flush: "sync" },
      );
      source.value = replacement;
      stop();
    }
    replacement = initial;
    run();
    dispatch(initial);
    dispatch(source.value);
  });
  assert.equal(hits, 0, "later-owner write executed before the call is not a replacement");
}

{
  const hits = await remainingHits(async (handler) => {
    const initial = new EventTarget();
    const source = ref(initial);
    let candidate = initial;
    candidate = initial;
    candidate.addEventListener = () => {};
    const stop = watch(
      source,
      (target, _prev, onCleanup) => {
        target.addEventListener("click", handler);
        onCleanup(() => {
          source.value.removeEventListener("click", handler);
        });
      },
      { immediate: true, flush: "sync" },
    );
    source.value = new EventTarget();
    stop();
    dispatch(initial);
    dispatch(source.value);
  });
  assert.equal(hits, 0, "method mutation through a written receiver alias installs no native listener");
}

{
  const hits = await remainingHits(async (handler) => {
    const initial = new EventTarget();
    const source = ref(initial);
    const fresh = new EventTarget();
    const middle = fresh;
    const replacement = middle;
    const stop = watch(
      source,
      (target, _prev, onCleanup) => {
        target.addEventListener("click", handler);
        onCleanup(() => {
          source.value.removeEventListener("click", handler);
        });
      },
      { immediate: true, flush: "sync" },
    );
    source.value = replacement;
    stop();
    dispatch(initial);
    dispatch(source.value);
  });
  assert.equal(hits, 1, "two-hop const payload alias of a distinct allocation leaks");
}

{
  const hits = await remainingHits(async (handler) => {
    const initial = new EventTarget();
    const source = ref(initial);
    let candidate = initial;
    candidate = initial;
    Reflect.set(candidate, "addEventListener", () => {});
    const stop = watch(
      source,
      (target, _prev, onCleanup) => {
        target.addEventListener("click", handler);
        onCleanup(() => {
          source.value.removeEventListener("click", handler);
        });
      },
      { immediate: true, flush: "sync" },
    );
    source.value = new EventTarget();
    stop();
    dispatch(initial);
    dispatch(source.value);
  });
  assert.equal(hits, 0, "generic escape of a written payload alias installs no native listener");
}

{
  const hits = await remainingHits(async (handler) => {
    const initial = new EventTarget();
    const source = ref(initial);
    let candidate;
    candidate = initial;
    Reflect.set(candidate, "addEventListener", () => {});
    const stop = watch(
      source,
      (target, _prev, onCleanup) => {
        target.addEventListener("click", handler);
        onCleanup(() => {
          source.value.removeEventListener("click", handler);
        });
      },
      { immediate: true, flush: "sync" },
    );
    source.value = new EventTarget();
    stop();
    dispatch(initial);
    dispatch(source.value);
  });
  assert.equal(hits, 0, "assignment of a native payload then generic escape installs no native listener");
}

{
  const hits = await remainingHits(async (handler) => {
    const initial = new EventTarget();
    const source = ref(initial);
    let candidate = initial;
    candidate = initial;
    const forwarded = candidate;
    Reflect.set(forwarded, "addEventListener", () => {});
    const stop = watch(
      source,
      (target, _prev, onCleanup) => {
        target.addEventListener("click", handler);
        onCleanup(() => {
          source.value.removeEventListener("click", handler);
        });
      },
      { immediate: true, flush: "sync" },
    );
    source.value = new EventTarget();
    stop();
    dispatch(initial);
    dispatch(source.value);
  });
  assert.equal(hits, 0, "copy of a written native payload then generic escape installs no native listener");
}

{
  const hits = await remainingHits(async (handler) => {
    const initial = new EventTarget();
    const source = ref(initial);
    function disable() {
      Reflect.set(candidate, "addEventListener", () => {});
    }
    const candidate = initial;
    disable();
    const stop = watch(
      source,
      (target, _prev, onCleanup) => {
        target.addEventListener("click", handler);
        onCleanup(() => {
          source.value.removeEventListener("click", handler);
        });
      },
      { immediate: true, flush: "sync" },
    );
    source.value = new EventTarget();
    stop();
    dispatch(initial);
    dispatch(source.value);
  });
  assert.equal(hits, 0, "escape before a later declaration installs no native listener");
}

console.log(
  JSON.stringify({
    vue: vue.version,
    id: "vue-vet/reactivity/no-watch-cleanup-current-source",
    flush: ["pre", "sync"],
    cleanupApis: ["onCleanup", "onWatcherCleanup"],
    leak: { afterChange: 1, afterStop: 2 },
    captured: { afterChange: 0, afterStop: 0 },
    schedule: {
      coalesced: 0,
      preBoundary: 1,
      onceImmediate: 0,
      stopped: 0,
      sameTargetAlias: 0,
      capturedRelease: 0,
      payloadMethod: 0,
      sameBeforeFirst: 0,
      preRoundTrip: 0,
      inactiveWatch: 0,
      stopAlias: 0,
      laterMutated: 0,
      nullHandler: 0,
      mutablePayloadAlias: 0,
      conditionalEarlierStop: 0,
      conditionalPayloadWrite: 0,
      deadPayloadWrite: 0,
      constPayloadAlias: 1,
      logicalPayloadWrite: 0,
      laterOwnerWrite: 0,
      writtenAliasMethod: 0,
      constChainAlias: 1,
      writtenAliasEscape: 0,
      assignedPayloadEscape: 0,
      copiedPayloadEscape: 0,
      laterDeclaredEscape: 0,
    },
  }),
);
