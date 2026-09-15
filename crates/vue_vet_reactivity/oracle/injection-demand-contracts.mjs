/**
 * Vue 3.5.40 runtime premises for same-instance injection demand.
 *
 * Locked oracle: this package's node_modules (Vue 3.5.40).
 * Run: `just oracle-injection-demand`
 */
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import path from "node:path";

const oraclePkg = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "package.json");
const requireVue = createRequire(oraclePkg);
const vue = requireVue("vue");
assert.equal(vue.version, "3.5.40", `expected Vue 3.5.40, got ${vue.version}`);

const { createApp, createSSRApp, h, inject, provide } = vue;
const { renderToString } = requireVue("vue/server-renderer");

let checks = 0;

function throws(fn, name) {
  let failed = false;
  let kind = null;
  try {
    fn();
  } catch (error) {
    failed = true;
    kind = error?.name ?? "Error";
    assert.equal(error instanceof TypeError, true, `${name} must be TypeError, got ${error}`);
  }
  assert.equal(failed, true, `${name} must throw`);
  checks += 1;
  return kind;
}

function check(name, actual, expected) {
  assert.deepEqual(actual, expected, name);
  checks += 1;
}

function errorKind(fn) {
  try {
    fn();
    return null;
  } catch (error) {
    return error?.name ?? "Error";
  }
}

async function render(setup, { parentValue, appValue } = {}) {
  let result;
  const Child = {
    setup() {
      result = setup();
      return () => h("span");
    },
  };
  const Root =
    parentValue === undefined
      ? Child
      : {
          setup() {
            provide("count", parentValue);
            return () => h(Child);
          },
        };
  const app = createSSRApp(Root);
  if (appValue !== undefined) app.provide("count", appValue);
  await renderToString(app);
  return result;
}

async function mount(setup, { parentValue, appValue } = {}) {
  let result;
  const Child = {
    setup() {
      result = setup();
      return () => h("span");
    },
  };
  const Root =
    parentValue === undefined
      ? Child
      : {
          setup() {
            provide("count", parentValue);
            return () => h(Child);
          },
        };
  const app = createApp(Root);
  if (appValue !== undefined) app.provide("count", appValue);
  await renderToString(app);
  return result;
}

{
  const actual = await render(() => {
    const key = Symbol("count");
    provide(key, 7);
    const count = inject(key, "missing");
    return { actual: count, demand: errorKind(() => count.toFixed(2)) };
  });
  check("ssr-fresh-symbol-fallback-fails-toFixed", actual, {
    actual: "missing",
    demand: "TypeError",
  });
}

{
  const actual = await mount(() => {
    const key = Symbol("count");
    provide(key, 7);
    const count = inject(key, "missing");
    return { actual: count, demand: errorKind(() => count.toFixed(2)) };
  });
  check("mounted-fresh-symbol-fallback-fails-toFixed", actual, {
    actual: "missing",
    demand: "TypeError",
  });
}

check(
  "ssr-string-key-consumes-ancestor",
  await render(
    () => {
      provide("count", 7);
      return inject("count", "missing").toFixed(2);
    },
    { parentValue: 9 },
  ),
  "9.00",
);

check(
  "ssr-string-key-consumes-app-provider",
  await render(
    () => {
      provide("count", 7);
      return inject("count", "missing").toFixed(2);
    },
    { appValue: 11 },
  ),
  "11.00",
);

check(
  "ssr-fresh-symbol-valid-number-fallback",
  await render(() => {
    const key = Symbol("count");
    provide(key, 7);
    return inject(key, 0).toFixed(2);
  }),
  "0.00",
);

check(
  "ssr-fresh-symbol-factory-fallback-fails-toFixed",
  await render(() => {
    const key = Symbol("count");
    provide(key, 7);
    const count = inject(key, () => "missing", true);
    return { actual: count, demand: errorKind(() => count.toFixed(2)) };
  }),
  { actual: "missing", demand: "TypeError" },
);

check(
  "ssr-local-value-still-works-in-setup",
  await render(() => {
    const key = Symbol("count");
    const local = 7;
    provide(key, local);
    return local.toFixed(2);
  }),
  "7.00",
);

throws(() => {
  const count = "missing";
  count.toFixed(2);
}, "string fallback toFixed is TypeError outside Vue");

throws(() => {
  const count = "missing";
  count?.toFixed(2);
}, "optional chain on string fallback still throws");

throws(() => {
  const count = "missing";
  count["toFixed"](2);
}, "computed string-literal member on string fallback throws");

{
  const actual = await render(() => {
    const key = Symbol("count");
    provide(key, 7);
    const count = inject(key, "missing");
    const label = inject(key, -1);
    return {
      optional: errorKind(() => count?.toFixed(2)),
      computed: errorKind(() => count["toFixed"](2)),
      unary: errorKind(() => label.toUpperCase()),
      erasedNonNull: errorKind(() => count.toFixed(2)),
    };
  });
  check("ssr-ts-erase-optional-computed-unary", actual, {
    optional: "TypeError",
    computed: "TypeError",
    unary: "TypeError",
    erasedNonNull: "TypeError",
  });
}

{
  const actual = await render(() => {
    const key = Symbol("count");
    provide(key, 7);
    const count = inject(key);
    return errorKind(() => count?.toFixed(2));
  });
  check("ssr-optional-on-nullish-fallback-is-silent", actual, null);
}

assert.equal(checks >= 12, true, `expected at least 12 semantic checks, got ${checks}`);
console.log(`injection-demand-contracts: ${checks} checks passed on Vue ${vue.version}`);
