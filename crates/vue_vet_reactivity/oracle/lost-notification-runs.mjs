/**
 * Vue 3.5.40 lost-notification premises for the two shipped IDs.
 *
 *   node lost-notification-runs.mjs
 */
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const requireVue = createRequire(fileURLToPath(new URL("./package.json", import.meta.url)));
const vue = requireVue("vue");
const {
  isReactive,
  isRef,
  isShallow,
  markRaw,
  reactive,
  shallowReactive,
  shallowRef,
  toRaw,
  triggerRef,
  watchEffect,
  watchSyncEffect,
} = vue;

assert.equal(vue.version, "3.5.40", `expected Vue 3.5.40, got ${vue.version}`);

{
  const state = shallowRef({ count: 1 });
  let runs = 0;
  const stop = watchSyncEffect(() => {
    void state.value.count;
    runs += 1;
  });
  assert.equal(runs, 1, "shallowRef nested: initial");
  state.value.count = 2;
  assert.equal(runs, 1, "shallowRef nested write does not notify");
  state.value = { count: 3 };
  assert.equal(runs, 2, "shallowRef slot replacement notifies");
  stop();
}

{
  const state = shallowReactive({ details: { count: 1 } });
  let runs = 0;
  const stop = watchSyncEffect(() => {
    void state.details.count;
    runs += 1;
  });
  assert.equal(runs, 1, "shallowReactive nested: initial");
  state.details.count = 2;
  assert.equal(runs, 1, "shallowReactive nested write does not notify");
  state.details = { count: 3 };
  assert.equal(runs, 2, "shallowReactive frontier replacement notifies");
  stop();
}

{
  const state = shallowReactive({ details: reactive({ count: 1 }) });
  let runs = 0;
  const stop = watchSyncEffect(() => {
    void state.details.count;
    runs += 1;
  });
  assert.equal(runs, 1, "nested reactive: initial");
  state.details.count = 2;
  assert.equal(runs, 2, "nested reactive child notifies");
  stop();
}

{
  const state = shallowRef({ count: 1 });
  let runs = 0;
  const stop = watchSyncEffect(() => {
    void state.value.count;
    runs += 1;
  });
  state.value.count = 2;
  triggerRef(state);
  assert.equal(runs, 2, "triggerRef repairs shallowRef nested write");
  stop();
}

{
  const state = reactive({ count: 1 });
  let runs = 0;
  const stop = watchSyncEffect(() => {
    void state.count;
    runs += 1;
  });
  assert.equal(runs, 1, "toRaw: initial");
  const raw = toRaw(state);
  raw.count = 2;
  assert.equal(runs, 1, "toRaw write does not notify");
  state.count = 3;
  assert.equal(runs, 2, "proxy write notifies");
  stop();
}

{
  const state = reactive({ count: 1 });
  let runs = 0;
  const stop = watchSyncEffect(() => {
    void toRaw(state).count;
    runs += 1;
  });
  const raw = toRaw(state);
  raw.count = 2;
  assert.equal(runs, 1, "raw-only consumer is not a proxy subscription");
  stop();
}

{
  const inactive = shallowRef({ n: 1 });
  let runs = 0;
  if (false) {
    watchSyncEffect(() => {
      void inactive.value.n;
      runs += 1;
    });
  }
  inactive.value.n = 2;
  assert.equal(runs, 0, "if(false) watcher never activates");
}

{
  const deferred = shallowRef({ n: 1 });
  let runs = 0;
  const stop = watchSyncEffect(async () => {
    await Promise.resolve();
    void deferred.value.n;
    runs += 1;
  });
  deferred.value.n = 2;
  await Promise.resolve();
  assert.equal(runs, 1, "async consumer first body runs after await, not as a live dep");
  deferred.value.n = 3;
  await Promise.resolve();
  assert.equal(runs, 1, "write after await-only read does not notify");
  stop();
}

{
  const stopped = shallowRef({ n: 1 });
  let runs = 0;
  const handle = watchSyncEffect(() => {
    void stopped.value.n;
    runs += 1;
  });
  assert.equal(runs, 1, "stopped handle: initial");
  handle.stop();
  stopped.value.n = 2;
  assert.equal(runs, 1, "handle.stop() permanently stops the consumer");
}

{
  const duplicateFlush = shallowRef({ n: 1 });
  let runs = 0;
  const stop = watchEffect(
    () => {
      void duplicateFlush.value.n;
      runs += 1;
    },
    { flush: "pre", flush: "post" },
  );
  duplicateFlush.value.n = 2;
  assert.equal(runs, 0, "duplicate flush last post has not run yet");
  await Promise.resolve();
  stop();
}

{
  const spreadFlush = shallowRef({ n: 1 });
  let runs = 0;
  const postOptions = { flush: "post" };
  const stop = watchEffect(
    () => {
      void spreadFlush.value.n;
      runs += 1;
    },
    { flush: "pre", ...postOptions },
  );
  spreadFlush.value.n = 2;
  assert.equal(runs, 0, "spread flush post has not run yet");
  await Promise.resolve();
  stop();
}

{
  const spreadArgs = shallowRef({ n: 1 });
  let runs = 0;
  const stop = watchEffect(() => {
    void spreadArgs.value.n;
    runs += 1;
  }, ...[{ flush: "post" }]);
  spreadArgs.value.n = 2;
  assert.equal(runs, 0, "spread call-argument flush post has not run yet");
  await Promise.resolve();
  stop();
}

{
  const state = reactive(markRaw({ n: 0 }));
  let runs = 0;
  const stop = watchSyncEffect(() => {
    void state.n;
    runs += 1;
  });
  assert.equal(runs, 1, "markRaw reactive: initial (no proxy deps)");
  toRaw(state).n = 1;
  assert.equal(runs, 1, "markRaw target write does not notify");
  stop();
}

{
  const replaced = shallowRef({ n: 0 });
  ({ x: replaced.value } = { x: reactive({ n: 0 }) });
  let runs = 0;
  const stop = watchSyncEffect(() => {
    void replaced.value.n;
    runs += 1;
  });
  replaced.value.n = 1;
  assert.equal(runs, 2, "assignment-pattern reactive payload notifies");
  stop();
}

{
  const aliased = shallowRef({ n: 0 });
  let mutableAlias = aliased;
  mutableAlias.value = reactive({ n: 0 });
  let runs = 0;
  const stop = watchSyncEffect(() => {
    void aliased.value.n;
    runs += 1;
  });
  aliased.value.n = 1;
  assert.equal(runs, 2, "mutable alias reactive payload notifies");
  stop();
}

{
  const spread = shallowRef({ item: { n: 0 }, ...{ item: reactive({ n: 0 }) } });
  let runs = 0;
  const stop = watchSyncEffect(() => {
    void spread.value.item.n;
    runs += 1;
  });
  spread.value.item.n = 1;
  assert.equal(runs, 2, "spread reactive child notifies");
  stop();
}

{
  const duplicate = shallowRef({
    item: { deep: { n: 0 } },
    item: reactive({ deep: { n: 0 } }),
  });
  let runs = 0;
  const stop = watchSyncEffect(() => {
    void duplicate.value.item.deep.n;
    runs += 1;
  });
  duplicate.value.item.deep.n = 1;
  assert.equal(runs, 2, "duplicate-key reactive subtree notifies");
  stop();
}

{
  const marked = reactive({ __v_skip: true, n: 0 });
  let runs = 0;
  const stop = watchSyncEffect(() => {
    void marked.n;
    runs += 1;
  });
  toRaw(marked).n = 1;
  assert.equal(runs, 1, "__v_skip object write does not notify");
  stop();
}

{
  const replacedBeforeWatch = shallowRef({ item: { n: 0 } });
  replacedBeforeWatch.value.item = reactive({ n: 0 });
  let runs = 0;
  const stop = watchSyncEffect(() => {
    void replacedBeforeWatch.value.item.n;
    runs += 1;
  });
  replacedBeforeWatch.value.item.n = 1;
  assert.equal(runs, 2, "nested payload replace with reactive child notifies");
  stop();
}

{
  const storedProxy = reactive({ item: reactive({ n: 0 }) });
  const outerRaw = toRaw(storedProxy);
  let runs = 0;
  const stop = watchSyncEffect(() => {
    void storedProxy.item.n;
    runs += 1;
  });
  outerRaw.item.n = 1;
  assert.equal(runs, 2, "toRaw outer then nested proxy member notifies");
  stop();
}

{
  const accessor = shallowRef({ n: 0 });
  let runs = 0;
  const stop = watchEffect(
    () => {
      void accessor.value.n;
      runs += 1;
    },
    {
      get flush() {
        return "post";
      },
    },
  );
  accessor.value.n = 1;
  assert.equal(runs, 0, "getter flush post has not run yet");
  await Promise.resolve();
  stop();
}

{
  const readonlyMarker = reactive({ __v_isReadonly: true, n: 0 });
  let runs = 0;
  const stop = watchSyncEffect(() => {
    void readonlyMarker.n;
    runs += 1;
  });
  toRaw(readonlyMarker).n = 1;
  assert.equal(runs, 1, "__v_isReadonly target write does not notify");
  stop();
}

{
  const rawMarker = reactive({ __v_raw: { n: 0 }, n: 0 });
  let runs = 0;
  const stop = watchSyncEffect(() => {
    void rawMarker.n;
    runs += 1;
  });
  toRaw(rawMarker).n = 1;
  assert.equal(runs, 1, "__v_raw target write does not notify");
  stop();
}

{
  const inheritedMarker = reactive({ __proto__: { __v_skip: true }, n: 0 });
  let runs = 0;
  const stop = watchSyncEffect(() => {
    void inheritedMarker.n;
    runs += 1;
  });
  toRaw(inheritedMarker).n = 1;
  assert.equal(runs, 1, "inherited __v_skip write does not notify");
  stop();
}

{
  const nestedMarker = reactive({ item: { __v_skip: true, n: 0 } });
  let runs = 0;
  const stop = watchSyncEffect(() => {
    void nestedMarker.item.n;
    runs += 1;
  });
  const rawNested = toRaw(nestedMarker);
  rawNested.item.n = 1;
  assert.equal(runs, 1, "nested __v_skip raw write does not notify");
  stop();
}

{
  const state = reactive({ __v_skip: 1, n: 0 });
  assert.equal(isReactive(state), false, "numeric __v_skip stays a plain object");
  let runs = 0;
  const stop = watchSyncEffect(() => {
    void state.n;
    runs += 1;
  });
  toRaw(state).n += 1;
  assert.equal(runs, 1, "numeric __v_skip write does not notify");
  stop();
}

{
  const state = reactive({ __v_isReadonly: "yes", n: 0 });
  assert.equal(isReactive(state), false, "string __v_isReadonly stays a plain object");
  let runs = 0;
  const stop = watchSyncEffect(() => {
    void state.n;
    runs += 1;
  });
  toRaw(state).n += 1;
  assert.equal(runs, 1, "string __v_isReadonly write does not notify");
  stop();
}

{
  const input = { __v_isRef: true, value: { n: 0 } };
  const state = shallowRef(input);
  assert.equal(state, input, "shallowRef returns the existing ref-tagged object");
  assert.equal(isRef(state), true, "normalized object isRef");
  assert.equal(isShallow(state), false, "normalized object is not a shallow ref");
  let runs = 0;
  const stop = watchSyncEffect(() => {
    void state.value.n;
    runs += 1;
  });
  state.value.n += 1;
  assert.equal(runs, 1, "normalized ref nested write does not notify");
  stop();
}

{
  const state = reactive({ __v_skip: false, n: 0 });
  assert.equal(isReactive(state), true, "false __v_skip still allocates a proxy");
  let runs = 0;
  const stop = watchSyncEffect(() => {
    void state.n;
    runs += 1;
  });
  toRaw(state).n = 1;
  assert.equal(runs, 1, "false __v_skip toRaw write does not notify");
  stop();
}

{
  const state = shallowRef({ __v_isRef: 1, value: { n: 0 } });
  assert.equal(isRef(state), true, "numeric __v_isRef still wraps");
  assert.equal(isShallow(state), true, "numeric __v_isRef creates a shallow ref");
  let runs = 0;
  const stop = watchSyncEffect(() => {
    void state.value.value.n;
    runs += 1;
  });
  state.value.value.n += 1;
  assert.equal(runs, 1, "numeric __v_isRef nested write does not notify");
  stop();
}

{
  const state = reactive({ n: 0 });
  assert.equal(isReactive(state), true, "missing flags allocate a proxy");
  let runs = 0;
  const stop = watchSyncEffect(() => {
    void state.n;
    runs += 1;
  });
  toRaw(state).n = 1;
  assert.equal(runs, 1, "ordinary toRaw write does not notify");
  stop();
}

console.log("lost-notification Vue 3.5.40 premises ok");
