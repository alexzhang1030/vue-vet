/**
 * Vue 3.5.40 + VueUse 13.9.0 runtime premises for until timeout unmatched demand.
 *
 * Resolves `vue` / `@vueuse/*` from this package lock only.
 * Uses short native timers; every promise settles.
 * Run: `just oracle-until-demand`
 */
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { fileURLToPath, pathToFileURL } from "node:url";
import path from "node:path";

const here = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(path.join(here, "package.json"));
const vue = await import(pathToFileURL(require.resolve("vue")));
const core = await import(pathToFileURL(require.resolve("@vueuse/core")));
const shared = await import(pathToFileURL(require.resolve("@vueuse/shared")));
assert.equal(require("vue/package.json").version, "3.5.40");
assert.equal(require("@vueuse/core/package.json").version, "13.9.0");
assert.equal(require("@vueuse/shared/package.json").version, "13.9.0");
assert.equal(core.until, shared.until);

const { ref } = vue;
const { until } = shared;

function resultOf(fn) {
  try {
    return { value: fn() };
  } catch (error) {
    return { error: error.name };
  }
}

let checks = 0;
function check(name, actual, expected) {
  assert.deepEqual(actual, expected, name);
  checks += 1;
}

{
  const source = ref(0);
  const value = await until(source).toBe("ready", { timeout: 5 });
  check("until-timeout-fulfills-current-unmatched-value", value, 0);
  check("until-timeout-expected-kind-demand-fails", resultOf(() => value.toUpperCase()), {
    error: "TypeError",
  });
  check("until-timeout-current-kind-demand-is-valid", value.toFixed(1), "0.0");
}

{
  const source = ref(0);
  const promise = until(source).toBe("ready", { timeout: 5 });
  source.value = "ready";
  check("until-source-update-before-timeout-satisfies-demand", (await promise).toUpperCase(), "READY");
}

{
  const rejected = await until(ref(0))
    .toBe("ready", { timeout: 5, throwOnTimeout: true })
    .then(
      () => "fulfilled",
      (error) => String(error),
    );
  check("until-explicit-timeout-error-handling-is-valid", rejected, "Timeout");
}

{
  const source = ref("ready");
  const value = await until(source).toBe("ready", { timeout: 5 });
  check("until-already-matched-returns-value", value.toUpperCase(), "READY");
}

{
  const source = ref(0);
  const value = await until(source).toBe("ready", { timeout: 0 });
  check("until-zero-timeout-fulfills-current-unmatched-value", value, 0);
  check("until-zero-timeout-expected-kind-demand-fails", resultOf(() => value.toUpperCase()), {
    error: "TypeError",
  });
}

{
  const source = ref(0);
  const value = await until(source).toBe("ready", { timeout: 5 });
  check("until-optional-chain-on-primitive-timeout-throws", resultOf(() => value?.toUpperCase()), {
    error: "TypeError",
  });
}

{
  const source = ref(0);
  const promise = until(source).toBe("ready", { timeout: 5 });
  source.value ||= "ready";
  check("until-compound-or-assign-before-await-matches", (await promise).toUpperCase(), "READY");
}

assert.equal(checks, 10, `expected 10 semantic checks, got ${checks}`);
console.log("until-demand oracle ok", { checks, vue: vue.version, vueuse: "13.9.0" });
