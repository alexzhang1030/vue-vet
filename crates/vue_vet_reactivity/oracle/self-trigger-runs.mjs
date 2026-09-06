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

{
  const queue = [];
  globalThis.requestAnimationFrame = (callback) => {
    queue.push(callback);
    return queue.length;
  };
  const flush = (count) => {
    for (let index = 0; index < count; index += 1) {
      const batch = queue.splice(0);
      for (const callback of batch) {
        callback();
      }
    }
  };

  let oneShot = 0;
  requestAnimationFrame(() => {
    oneShot += 1;
  });
  flush(3);
  assert.equal(oneShot, 1, "one-shot rAF runs once");

  let twoFrame = 0;
  function render() {
    twoFrame += 1;
  }
  requestAnimationFrame(() => {
    requestAnimationFrame(render);
  });
  flush(1);
  assert.equal(twoFrame, 0, "two-frame delay has not run after one frame");
  flush(1);
  assert.equal(twoFrame, 1, "two-frame delay runs on the second frame");
  flush(3);
  assert.equal(twoFrame, 1, "two-frame delay does not repeat");

  let repeating = 0;
  function tick() {
    repeating += 1;
    if (repeating < 4) {
      requestAnimationFrame(tick);
    }
  }
  requestAnimationFrame(tick);
  flush(4);
  assert.equal(repeating, 4, "self-scheduling callback repeats");
  results.push({
    id: "raf-finite-versus-repeating",
    oneShot,
    twoFrame,
    repeating,
  });
}

{
  const vueFull = requireVue("vue/dist/vue.cjs.js");
  const node = (tag, text = "") => ({ tag, text, children: [], parent: null });
  function remove(child) {
    if (child.parent) {
      child.parent.children.splice(child.parent.children.indexOf(child), 1);
    }
    child.parent = null;
  }
  const renderer = vueFull.createRenderer({
    createElement: (tag) => node(tag),
    createText: (text) => node("#text", text),
    createComment: (text) => node("#comment", text),
    setText: (target, text) => {
      target.text = text;
    },
    setElementText: (target, text) => {
      target.text = text;
      target.children = [];
    },
    parentNode: (target) => target.parent,
    nextSibling: (target) =>
      target.parent?.children[target.parent.children.indexOf(target) + 1] ?? null,
    patchProp: () => {},
    insert(child, parent, anchor = null) {
      remove(child);
      const index = anchor ? parent.children.indexOf(anchor) : parent.children.length;
      parent.children.splice(index, 0, child);
      child.parent = parent;
    },
    remove,
  });

  const SlotList = {
    props: ["items"],
    render: vueFull.compile(
      `<div><slot v-for="item in items" v-bind="{ key: item.id }" :item="item" /></div>`,
    ),
  };
  const slots = vueFull.h(
    SlotList,
    { items: [{ id: "first" }, { id: "second" }] },
    { default: ({ item }) => vueFull.h("span", item.id) },
  );
  renderer.render(slots, node("root"));
  function keys(vnode) {
    if (!vnode || typeof vnode !== "object") {
      return [];
    }
    return [
      ...(vnode.key == null ? [] : [vnode.key]),
      ...(Array.isArray(vnode.children) ? vnode.children.flatMap(keys) : []),
    ];
  }
  const slotKeys = keys(slots.component.subTree);
  assert.deepEqual(slotKeys, ["first", "second"], "object-form v-bind key must appear on slot VNodes");
  results.push({ id: "object-form-slot-key", slotKeys });

  const visible = vueFull.ref(false);
  const events = [];
  const ToggleChild = {
    setup: () => () => (visible.value ? vueFull.h("p", "shown") : vueFull.createCommentVNode("")),
  };
  const Parent = {
    setup: () => () =>
      vueFull.h(
        vueFull.Transition,
        {
          css: false,
          onEnter: (_element, done) => {
            events.push("enter");
            done();
          },
          onLeave: (_element, done) => {
            events.push("leave");
            done();
          },
        },
        { default: () => vueFull.h(ToggleChild) },
      ),
  };
  const root = node("root");
  renderer.render(vueFull.h(Parent), root);
  visible.value = true;
  await vueFull.nextTick();
  visible.value = false;
  await vueFull.nextTick();
  assert.deepEqual(events, ["enter", "leave"], "Transition forwards enter/leave into a child-root toggle");
  renderer.render(null, root);
  results.push({ id: "child-root-transition", transitionEvents: events });
}

const report = {
  vue: vue.version,
  oracleRoot: path.dirname(fileURLToPath(import.meta.url)),
  kind: "run-count",
  results,
};
process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
