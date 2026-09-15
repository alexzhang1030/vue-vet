/**
 * Vue 3.5.40 customRef notification-chain premises.
 *
 * Locked oracle: this package's node_modules (Vue 3.5.40).
 * Run: `just oracle-custom-ref-notification`
 *
 * Explicit run counts (watch/effect callbacks, flush: "sync"):
 *   lostTrack / lostTrigger / trackInSetter → extra runs = 0
 *   standard / backingRef / helper → extra runs = 1
 *   deferredTrigger → before = 0, after stored trigger = 1
 *   triggerRef (track in get, no trigger in set, then triggerRef) → extra runs = 1
 *   sameValue / noConsumer / stop / pause stay at the initial subscription
 */
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import path from "node:path";

const oraclePkg = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "package.json");
const requireVue = createRequire(oraclePkg);
const vue = requireVue("vue");
assert.equal(vue.version, "3.5.40", `expected Vue 3.5.40, got ${vue.version}`);

const { customRef, ref, reactive, triggerRef, watch, watchEffect, watchSyncEffect, watchPostEffect, nextTick } = vue;

function lostTrackFactory() {
  return customRef((_track, trigger) => {
    let value = 0;
    return {
      get() {
        return value;
      },
      set(next) {
        value = next;
        trigger();
      },
    };
  });
}

function lostTriggerFactory() {
  return customRef((track, _trigger) => {
    let value = 0;
    return {
      get() {
        track();
        return value;
      },
      set(next) {
        value = next;
      },
    };
  });
}

function trackInSetterFactory() {
  return customRef((track, trigger) => {
    let value = 0;
    return {
      get() {
        return value;
      },
      set(next) {
        track();
        value = next;
        trigger();
      },
    };
  });
}

function standardFactory() {
  return customRef((track, trigger) => {
    let value = 0;
    return {
      get() {
        track();
        return value;
      },
      set(next) {
        value = next;
        trigger();
      },
    };
  });
}

const counts = {};

{
  const r = lostTrackFactory();
  let runs = 0;
  const stop = watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  counts.lostTrack = { runs, value: r.value };
  assert.equal(runs, 0, "lost track: subscribed watch must not re-run");
  assert.equal(r.value, 1, "lost track: the write still stores");
  stop();
}

{
  const r = lostTriggerFactory();
  let runs = 0;
  const stop = watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  counts.lostTrigger = { runs, value: r.value };
  assert.equal(runs, 0, "lost trigger: subscribed watch must not re-run");
  assert.equal(r.value, 1, "lost trigger: the write still stores");
  stop();
}

{
  const r = trackInSetterFactory();
  let runs = 0;
  const stop = watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  counts.trackInSetter = { runs, value: r.value };
  assert.equal(runs, 0, "track only in setter: get never subscribes");
  stop();
}

{
  const r = standardFactory();
  let runs = 0;
  const stop = watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  counts.standard = { runs, value: r.value };
  assert.equal(runs, 1, "standard customRef notifies once");
  stop();
}

{
  const backing = ref(0);
  const r = customRef(() => ({
    get() {
      return backing.value;
    },
    set(next) {
      backing.value = next;
    },
  }));
  let hits = 0;
  const stop = watch(
    r,
    () => {
      hits += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  counts.backingRef = { hits, value: r.value, backing: backing.value };
  assert.equal(hits, 1, "backing ref bridge notifies without explicit track/trigger");
  assert.equal(backing.value, 1);
  stop();
}

{
  const backing = reactive({ n: 0 });
  const r = customRef(() => ({
    get() {
      return backing.n;
    },
    set(next) {
      backing.n = next;
    },
  }));
  let hits = 0;
  const stop = watch(
    r,
    () => {
      hits += 1;
    },
    { flush: "sync" },
  );
  r.value = 2;
  counts.backingReactive = { hits, value: r.value };
  assert.equal(hits, 1, "backing reactive bridge notifies without explicit track/trigger");
  stop();
}

{
  let notify;
  const r = customRef((track, trigger) => {
    let value = 0;
    return {
      get() {
        track();
        return value;
      },
      set(next) {
        value = next;
        notify = trigger;
      },
    };
  });
  let runs = 0;
  const stop = watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  const before = runs;
  notify();
  counts.deferredTrigger = { before, after: runs };
  assert.equal(before, 0, "saved trigger does not fire during set");
  assert.equal(runs, 1, "invoking the stored trigger notifies");
  stop();
}

{
  let notify;
  const r = customRef((track, trigger) => {
    let value = 0;
    return {
      get() {
        track();
        return value;
      },
      set(next) {
        value = next;
        notify = trigger;
      },
    };
  });
  let runs = 0;
  const stop = watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  const before = runs;
  setTimeout(() => {
    notify();
  }, 0);
  counts.deferredTimerArmed = { before };
  assert.equal(before, 0, "timer-scheduled trigger has not run yet");
  stop();
}

{
  const r = lostTriggerFactory();
  let runs = 0;
  const stop = watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  assert.equal(runs, 0, "pre-triggerRef: setter did not notify");
  triggerRef(r);
  counts.triggerRef = { runs, value: r.value };
  assert.equal(runs, 1, "triggerRef after a no-trigger set notifies existing subscribers");
  stop();
}

{
  const r = lostTrackFactory();
  let runs = 0;
  const stop = watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  triggerRef(r);
  counts.triggerRefWithoutTrack = { runs };
  assert.equal(runs, 0, "triggerRef cannot notify when get never tracked");
  stop();
}

{
  const r = standardFactory();
  let runs = 0;
  const stop = watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 0;
  counts.sameValue = { runs, value: r.value };
  assert.equal(runs, 0, "same-value write does not notify");
  stop();
}

{
  const r = lostTrackFactory();
  r.value = 1;
  counts.noConsumer = { value: r.value };
  assert.equal(r.value, 1, "no consumer: write still stores; nothing to notify");
}

{
  const r = standardFactory();
  let runs = 0;
  const stop = watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  stop();
  r.value = 1;
  counts.stop = { runs, value: r.value };
  assert.equal(runs, 0, "stopped consumer does not re-run");
}

{
  const r = standardFactory();
  let runs = 0;
  const handle = watchEffect(
    () => {
      void r.value;
      runs += 1;
    },
    { flush: "sync" },
  );
  const afterSubscribe = runs;
  handle.pause();
  r.value = 1;
  counts.pause = { afterSubscribe, runs, value: r.value };
  assert.equal(afterSubscribe, 1, "watchEffect runs once on subscribe");
  assert.equal(runs, 1, "paused consumer does not re-run");
  handle.stop();
}

{
  function delegate(track, trigger) {
    let value = 0;
    return {
      get() {
        track();
        return value;
      },
      set(next) {
        value = next;
        trigger();
      },
    };
  }
  const r = customRef((track, trigger) => delegate(track, trigger));
  let runs = 0;
  const stop = watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  counts.helper = { runs, value: r.value };
  assert.equal(runs, 1, "helper that actually calls track/trigger still notifies");
  stop();
}

{
  const factory = (track, trigger) => {
    let value = 0;
    return {
      get() {
        track();
        return value;
      },
      set(next) {
        value = next;
        trigger();
      },
    };
  };
  const r = customRef(factory);
  let runs = 0;
  const stop = watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  counts.unknownFactory = { runs };
  assert.equal(runs, 1, "non-inline factory can still notify when it tracks/triggers");
  stop();
}

{
  const r = standardFactory();
  let effectRuns = 0;
  const stop = watchSyncEffect(() => {
    void r.value;
    effectRuns += 1;
  });
  const afterSubscribe = effectRuns;
  r.value = 1;
  counts.watchSyncEffect = { afterSubscribe, runs: effectRuns };
  assert.equal(afterSubscribe, 1);
  assert.equal(effectRuns, 2, "watchSyncEffect re-runs after a notifying set");
  stop();
}

{
  const r = lostTriggerFactory();
  let runs = 0;
  let observed = null;
  watchPostEffect(() => {
    observed = r.value;
    runs += 1;
  });
  r.value = 1;
  await nextTick();
  counts.postInitial = { runs, observed };
  assert.equal(runs, 1, "watchPostEffect first runs after the write");
  assert.equal(observed, 1);
}

{
  const r = lostTriggerFactory();
  let runs = 0;
  false &&
    watch(
      r,
      () => {
        runs += 1;
      },
      { flush: "sync" },
    );
  r.value = 1;
  counts.guardedConsumer = { runs, value: r.value };
  assert.equal(runs, 0, "guarded watch installs zero consumers");
}

{
  const r = lostTriggerFactory();
  let runs = 0;
  watch(
    r,
    () => {
      runs += 1;
    },
    { immediate: true, once: true, flush: "sync" },
  );
  const afterSubscribe = runs;
  r.value = 1;
  counts.onceImmediate = { afterSubscribe, runs, value: r.value };
  assert.equal(afterSubscribe, 1, "once+immediate runs on subscribe");
  assert.equal(runs, 1, "once+immediate is stopped before the later write");
}

{
  const r = lostTriggerFactory();
  let observed = null;
  watchEffect(
    () => {
      return;
      observed = r.value;
    },
    { flush: "sync" },
  );
  r.value = 1;
  counts.earlyReturn = { observed, value: r.value };
  assert.equal(observed, null, "effect return before the ref read leaves it unexecuted");
}

{
  const r = lostTriggerFactory();
  r.value = 1;
  let runs = 0;
  watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  counts.priorValue = { runs, value: r.value };
  assert.equal(runs, 0, "write then subscribe then same-value write does not notify");
}

{
  const r = customRef((track, trigger) => {
    let value = 0;
    return {
      get() {
        track();
        return value;
      },
      set() {
        value = 0;
      },
    };
  });
  let runs = 0;
  watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  counts.constantSetter = { runs, value: r.value };
  assert.equal(runs, 0, "setter that stores a constant does not change the observable value");
  assert.equal(r.value, 0);
}

{
  const r = customRef((track, trigger) => {
    let value = 0;
    return {
      get() {
        track();
        return value;
      },
      set(next) {
        if (value == next) return;
        value = next;
        trigger();
      },
    };
  });
  let runs = 0;
  watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = false;
  counts.coercingEquality = { runs, value: r.value };
  assert.equal(runs, 0, "coercing == can skip mutation of 0 vs false");
  assert.equal(r.value, 0);
}

{
  const r = customRef((track, trigger) => {
    let value = 0;
    const hooks = { track, trigger };
    return {
      get() {
        hooks.track();
        return value;
      },
      set(next) {
        value = next;
        hooks.trigger();
      },
    };
  });
  let runs = 0;
  watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  counts.memberDelegation = { runs, value: r.value };
  assert.equal(runs, 1, "member-delegated track/trigger still notifies");
}

{
  const r = customRef((track, trigger) => {
    let value = 0;
    return {
      get() {
        (() => track())();
        return value;
      },
      set(next) {
        value = next;
        trigger();
      },
    };
  });
  let runs = 0;
  watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  counts.iifeTracking = { runs, value: r.value };
  assert.equal(runs, 1, "IIFE-invoked track still notifies");
}

{
  const backing = ref(0);
  const r = customRef((track, trigger) => {
    let value = 0;
    return {
      get() {
        return value;
      },
      set(next) {
        value = next;
      },
    };
  });
  r._get = () => backing.value;
  r._set = (next) => {
    backing.value = next;
  };
  let runs = 0;
  watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  counts.capabilityWrite = { runs, value: r.value };
  assert.equal(runs, 1, "replacing get/set with a backing-ref bridge notifies");
  assert.equal(r.value, 1);
}

{
  const r = customRef((track, trigger) => {
    let value = 0;
    const hooks = { track, trigger };
    const delegate = hooks;
    return {
      get() {
        delegate.track();
        return value;
      },
      set(next) {
        value = next;
        delegate.trigger();
      },
    };
  });
  let runs = 0;
  watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  counts.holderAlias = { runs, value: r.value };
  assert.equal(runs, 1, "const-alias of a capability holder still notifies");
  assert.equal(r.value, 1);
}

{
  const r = customRef((track, trigger) => {
    let value = 0;
    return {
      get() {
        track();
        return value;
      },
      set(next) {
        value *= next;
      },
    };
  });
  let runs = 0;
  watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  counts.compoundSetter = { runs, value: r.value };
  assert.equal(runs, 0, "compound assignment does not identity-store the input");
  assert.equal(r.value, 0);
}

{
  const r = customRef((track, trigger) => {
    let value = 0;
    return {
      get() {
        track();
        value = 0;
        return value;
      },
      set(next) {
        value = next;
      },
    };
  });
  let runs = 0;
  watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  counts.getterStorageWrite = { runs, value: r.value };
  assert.equal(runs, 0, "getter storage write erases the setter store before observe");
  assert.equal(r.value, 0);
}

{
  const r = customRef((track, trigger) => {
    let value = 0;
    const initialized = (value = 1);
    return {
      get() {
        track();
        return value;
      },
      set(next) {
        value = next;
      },
    };
  });
  let runs = 0;
  watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  counts.factoryStorageWrite = { runs, value: r.value };
  assert.equal(runs, 0, "factory write of 1 then input 1 is the same observable value");
  assert.equal(r.value, 1);
}

{
  const r = customRef((track, trigger) => {
    let value = 0;
    return {
      get() {
        track();
        return value;
      },
      set(next) {
        next = 0;
        value = next;
      },
    };
  });
  let runs = 0;
  watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  counts.setterParameterWrite = { runs, value: r.value };
  assert.equal(runs, 0, "mutating the setter parameter before store keeps 0");
  assert.equal(r.value, 0);
}

{
  const r = customRef((track, trigger) => {
    let value = 0;
    return {
      get() {
        track();
        return value;
      },
      set(next) {
        value = next;
      },
    };
  });
  let runs = 0;
  let observed = null;
  watchEffect(async () => {
    await Promise.resolve();
    runs += 1;
    observed = r.value;
  });
  r.value = 1;
  await nextTick();
  counts.afterAwaitConsumer = { runs, observed };
  assert.equal(runs, 1, "watchEffect continuation after await runs once");
  assert.equal(observed, 1, "the post-await read observes the later write");
}

{
  const r = customRef((track, trigger) => {
    let value = 0;
    return {
      get() {
        const result = { value: (value = 0) };
        track();
        return value;
      },
      set(next) {
        value = next;
      },
    };
  });
  let runs = 0;
  watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  counts.getterObjectWrite = { runs, value: r.value };
  assert.equal(runs, 0, "getter object-value write resets storage before observe");
  assert.equal(r.value, 0);
}

{
  const r = customRef((track, trigger) => {
    let value = 0;
    return {
      get() {
        track();
        return value;
      },
      set(next) {
        const result = { [next = 0]: true };
        value = next;
      },
    };
  });
  let runs = 0;
  watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  counts.setterComputedKey = { runs, value: r.value };
  assert.equal(runs, 0, "computed-key assignment mutates the setter parameter");
  assert.equal(r.value, 0);
}

{
  const r = customRef((track, trigger) => {
    let value = 0;
    const initialized = { [value = 1]: true };
    void initialized;
    return {
      get() {
        track();
        return value;
      },
      set(next) {
        value = next;
      },
    };
  });
  let runs = 0;
  watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  counts.factoryComputedKey = { runs, value: r.value };
  assert.equal(runs, 0, "factory computed-key write of 1 then input 1 is the same value");
  assert.equal(r.value, 1);
}

{
  const r = customRef((track, trigger) => {
    let value = 0;
    const initialized = true ? (value = 1) : (value = 0);
    void initialized;
    return {
      get() {
        track();
        return value;
      },
      set(next) {
        value = next;
      },
    };
  });
  let runs = 0;
  watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  counts.factoryConditionalWrite = { runs, value: r.value };
  assert.equal(runs, 0, "proven true branch writes 1; later input 1 is unchanged");
  assert.equal(r.value, 1);
}

{
  const r = customRef((track, trigger) => {
    let value = 0;
    const initialized = true && (value = 1);
    void initialized;
    return {
      get() {
        track();
        return value;
      },
      set(next) {
        value = next;
      },
    };
  });
  let runs = 0;
  watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  counts.factoryLogicalWrite = { runs, value: r.value };
  assert.equal(runs, 0, "proven true && write of 1 then input 1 is unchanged");
  assert.equal(r.value, 1);
}

{
  const r = customRef((track, trigger) => {
    let value = 0;
    return {
      get() {
        track();
        return value;
      },
      *set(next) {
        value = next;
      },
    };
  });
  let runs = 0;
  watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  counts.generatorSetter = { runs, value: r.value };
  assert.equal(runs, 0, "generator setter body stays dormant on invocation");
  assert.equal(r.value, 0);
}

{
  const r = customRef((track, trigger) => {
    let value = 0;
    return {
      get() {
        track();
        return value;
      },
      set(next) {
        value = next;
      },
    };
  });
  let runs = 0;
  let observed = null;
  watchEffect(async () => {
    const resolved = { [await Promise.resolve()]: true };
    void resolved;
    runs += 1;
    observed = r.value;
  });
  r.value = 1;
  await nextTick();
  counts.afterComputedAwait = { runs, observed };
  assert.equal(runs, 1, "computed-key await ends the subscribed prefix");
  assert.equal(observed, 1, "the post-await read observes the later write");
}

{
  const r = customRef((track, trigger) => {
    let value = 0;
    const { supplied = (value = 1) } = { supplied: true };
    void supplied;
    return {
      get() {
        track();
        return value;
      },
      set(next) {
        value = next;
      },
    };
  });
  let runs = 0;
  watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 0;
  counts.factoryDormantDefault = { runs, value: r.value };
  assert.equal(runs, 0, "supplied binding default stays dormant");
  assert.equal(r.value, 0);
}

{
  const r = customRef((track, trigger) => {
    let value = 0;
    const { [value = 0]: supplied } = (value = 1, {});
    void supplied;
    return {
      get() {
        track();
        return value;
      },
      set(next) {
        value = next;
      },
    };
  });
  let runs = 0;
  watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 0;
  counts.factoryBindingOrder = { runs, value: r.value };
  assert.equal(runs, 0, "initializer runs before the computed binding key");
  assert.equal(r.value, 0);
}

{
  const r = customRef((track, trigger) => {
    let value = 0;
    const undefined = 7;
    const initialized = undefined ?? (value = 1);
    void initialized;
    return {
      get() {
        track();
        return value;
      },
      set(next) {
        value = next;
      },
    };
  });
  let runs = 0;
  watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 0;
  counts.factoryShadowedUndefined = { runs, value: r.value };
  assert.equal(runs, 0, "shadowed undefined is not nullish");
  assert.equal(r.value, 0);
}

{
  const r = customRef((track, trigger) => {
    let value = 0;
    return {
      get() {
        const result = { Reset: class { static { value = 0 } } };
        void result;
        track();
        return value;
      },
      set(next) {
        value = next;
      },
    };
  });
  let runs = 0;
  watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  counts.getterObjectClass = { runs, value: r.value };
  assert.equal(runs, 0, "class static block resets storage during getter evaluation");
  assert.equal(r.value, 0);
}

{
  const r = customRef((track, trigger) => {
    let value = 0;
    return {
      get() {
        track();
        return value;
      },
      set(next) {
        value = next;
      },
    };
  });
  let runs = 0;
  let observed = null;
  watchEffect(() => {
    runs += 1;
    {
      return;
    }
    observed = r.value;
  });
  r.value = 1;
  counts.nestedBlockReturn = { runs, observed };
  assert.equal(runs, 1, "effect runs once then returns from the nested block");
  assert.equal(observed, null, "the later read is not subscribed");
}

{
  const r = customRef((track, trigger) => {
    let value = 0;
    const { supplied = (value = 1) } = { supplied: undefined };
    void supplied;
    return {
      get() {
        track();
        return value;
      },
      set(next) {
        value = next;
      },
    };
  });
  let runs = 0;
  watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 1;
  counts.factoryExplicitUndefined = { runs, value: r.value };
  assert.equal(runs, 0, "explicit undefined activates the destructuring default");
  assert.equal(r.value, 1);
}

{
  const r = customRef((track, trigger) => {
    let value = 0;
    const { supplied = (value = 1), [value = 0]: unused } = {};
    void unused;
    return {
      get() {
        track();
        return value;
      },
      set(next) {
        value = next;
      },
    };
  });
  let runs = 0;
  watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 0;
  counts.factoryInterleavedKey = { runs, value: r.value };
  assert.equal(runs, 0, "each binding property runs key then default then the next property");
  assert.equal(r.value, 0);
}

{
  const r = customRef((track, trigger) => {
    let value = 0;
    const { missing: { leaf = (value = 1) } = { leaf: true } } = {};
    void leaf;
    return {
      get() {
        track();
        return value;
      },
      set(next) {
        value = next;
      },
    };
  });
  let runs = 0;
  watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 0;
  counts.factoryNestedDefaultSource = { runs, value: r.value };
  assert.equal(runs, 0, "activated default object supplies nested properties");
  assert.equal(r.value, 0);
}

{
  const r = customRef((track, trigger) => {
    let value = 0;
    const { supplied = (value = 1) } = { __proto__: { supplied: true } };
    void supplied;
    return {
      get() {
        track();
        return value;
      },
      set(next) {
        value = next;
      },
    };
  });
  let runs = 0;
  watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 0;
  counts.factoryInheritedProperty = { runs, value: r.value };
  assert.equal(runs, 0, "object-literal proto setter participates in lookup");
  assert.equal(r.value, 0);
}

{
  const r = customRef((track, trigger) => {
    let value = 0;
    const { toString = (value = 1) } = {};
    void toString;
    return {
      get() {
        track();
        return value;
      },
      set(next) {
        value = next;
      },
    };
  });
  let runs = 0;
  watch(
    r,
    () => {
      runs += 1;
    },
    { flush: "sync" },
  );
  r.value = 0;
  counts.factoryStandardPrototypeProperty = { runs, value: r.value };
  assert.equal(runs, 0, "standard prototype keys are not proven missing");
  assert.equal(r.value, 0);
}

assert.equal(counts.lostTrack.runs, 0);
assert.equal(counts.lostTrigger.runs, 0);
assert.equal(counts.trackInSetter.runs, 0);
assert.equal(counts.standard.runs, 1);
assert.equal(counts.backingRef.hits, 1);
assert.equal(counts.backingReactive.hits, 1);
assert.equal(counts.deferredTrigger.before, 0);
assert.equal(counts.deferredTrigger.after, 1);
assert.equal(counts.triggerRef.runs, 1);
assert.equal(counts.triggerRefWithoutTrack.runs, 0);
assert.equal(counts.sameValue.runs, 0);
assert.equal(counts.stop.runs, 0);
assert.equal(counts.pause.runs, 1);
assert.equal(counts.helper.runs, 1);
assert.equal(counts.unknownFactory.runs, 1);
assert.equal(counts.watchSyncEffect.runs, 2);
assert.equal(counts.postInitial.runs, 1);
assert.equal(counts.guardedConsumer.runs, 0);
assert.equal(counts.onceImmediate.runs, 1);
assert.equal(counts.earlyReturn.observed, null);
assert.equal(counts.priorValue.runs, 0);
assert.equal(counts.constantSetter.runs, 0);
assert.equal(counts.coercingEquality.runs, 0);
assert.equal(counts.memberDelegation.runs, 1);
assert.equal(counts.iifeTracking.runs, 1);
assert.equal(counts.capabilityWrite.runs, 1);
assert.equal(counts.holderAlias.runs, 1);
assert.equal(counts.compoundSetter.runs, 0);
assert.equal(counts.getterStorageWrite.runs, 0);
assert.equal(counts.factoryStorageWrite.runs, 0);
assert.equal(counts.setterParameterWrite.runs, 0);
assert.equal(counts.afterAwaitConsumer.runs, 1);
assert.equal(counts.afterAwaitConsumer.observed, 1);
assert.equal(counts.getterObjectWrite.runs, 0);
assert.equal(counts.getterObjectWrite.value, 0);
assert.equal(counts.setterComputedKey.runs, 0);
assert.equal(counts.setterComputedKey.value, 0);
assert.equal(counts.factoryComputedKey.runs, 0);
assert.equal(counts.factoryComputedKey.value, 1);
assert.equal(counts.factoryConditionalWrite.runs, 0);
assert.equal(counts.factoryConditionalWrite.value, 1);
assert.equal(counts.factoryLogicalWrite.runs, 0);
assert.equal(counts.factoryLogicalWrite.value, 1);
assert.equal(counts.generatorSetter.runs, 0);
assert.equal(counts.generatorSetter.value, 0);
assert.equal(counts.afterComputedAwait.runs, 1);
assert.equal(counts.afterComputedAwait.observed, 1);
assert.equal(counts.factoryDormantDefault.runs, 0);
assert.equal(counts.factoryDormantDefault.value, 0);
assert.equal(counts.factoryBindingOrder.runs, 0);
assert.equal(counts.factoryBindingOrder.value, 0);
assert.equal(counts.factoryShadowedUndefined.runs, 0);
assert.equal(counts.factoryShadowedUndefined.value, 0);
assert.equal(counts.getterObjectClass.runs, 0);
assert.equal(counts.getterObjectClass.value, 0);
assert.equal(counts.nestedBlockReturn.runs, 1);
assert.equal(counts.nestedBlockReturn.observed, null);
assert.equal(counts.factoryExplicitUndefined.runs, 0);
assert.equal(counts.factoryExplicitUndefined.value, 1);
assert.equal(counts.factoryInterleavedKey.runs, 0);
assert.equal(counts.factoryInterleavedKey.value, 0);
assert.equal(counts.factoryNestedDefaultSource.runs, 0);
assert.equal(counts.factoryNestedDefaultSource.value, 0);
assert.equal(counts.factoryInheritedProperty.runs, 0);
assert.equal(counts.factoryInheritedProperty.value, 0);
assert.equal(counts.factoryStandardPrototypeProperty.runs, 0);
assert.equal(counts.factoryStandardPrototypeProperty.value, 0);

console.log(
  JSON.stringify(
    {
      vue: vue.version,
      counts,
    },
    null,
    2,
  ),
);
console.log("custom-ref-notification oracle ok");
