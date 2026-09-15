/**
 * Vue 3.5.40 / VueUse 13.9.0 runtime premises for cancelled-filter demand.
 *
 * Locked oracle: this package's node_modules.
 * Native timers only; await every outstanding promise.
 * Run: `just oracle-filter-settlement`
 */
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import path from "node:path";

const oraclePkg = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "package.json");
const requirePkg = createRequire(oraclePkg);
const vue = requirePkg("vue");
const core = await import(requirePkg.resolve("@vueuse/core/index.mjs"));
const shared = await import(requirePkg.resolve("@vueuse/shared/index.mjs"));
assert.equal(vue.version, "3.5.40", `expected Vue 3.5.40, got ${vue.version}`);
assert.equal(requirePkg("@vueuse/core/package.json").version, "13.9.0");
assert.equal(requirePkg("@vueuse/shared/package.json").version, "13.9.0");
assert.equal(core.useDebounceFn, shared.useDebounceFn);
assert.equal(typeof shared.useDebounceFn, "function");
assert.equal(typeof shared.useThrottleFn, "function");

const { useDebounceFn, useThrottleFn } = shared;

let checks = 0;
function throws(fn, name) {
  let failed = false;
  try {
    fn();
  } catch (error) {
    failed = true;
    assert.equal(error instanceof TypeError, true, `${name} must be TypeError, got ${error}`);
  }
  assert.equal(failed, true, `${name} must throw`);
  checks += 1;
}

function check(name, actual, expected) {
  assert.deepEqual(actual, expected, name);
  checks += 1;
}

{
  const run = useDebounceFn((value) => value.toUpperCase(), 10);
  const first = run("one");
  const second = run("two");
  const cancelled = await first;
  const latest = await second;
  check("debounce-synchronous-supersession-fulfills-undefined", typeof cancelled, "undefined");
  check("debounce-latest-default-result", latest, "TWO");
  throws(() => cancelled.slice(0, 1), "cancelled debounce result must fail slice");
}

{
  const run = useDebounceFn((value) => value.toUpperCase(), 10);
  const first = run("one");
  run("two");
  const cancelled = await first;
  check("debounce-guarded-cancelled-value-is-valid", cancelled === undefined ? "cancelled" : cancelled.slice(0, 1), "cancelled");
}

{
  const run = useDebounceFn((value) => value.toUpperCase(), 10, { rejectOnCancel: true });
  const first = run("one").then(
    (value) => ({ value }),
    (error) => ({ rejected: true, errorType: typeof error }),
  );
  const second = run("two");
  check("debounce-rejection-control-has-explicit-error-path", await first, {
    rejected: true,
    errorType: "undefined",
  });
  check("debounce-latest-rejection-control-fulfills", await second, "TWO");
}

{
  const run = useDebounceFn((value) => value.toUpperCase(), 0);
  check("debounce-zero-duration-preserves-both-results", await Promise.all([run("one"), run("two")]), [
    "ONE",
    "TWO",
  ]);
}

{
  const sequential = useDebounceFn((value) => value.toUpperCase(), 5);
  const first = await sequential("one");
  const second = await sequential("two");
  check("debounce-sequential-waits-preserve-both-results", [first, second], ["ONE", "TWO"]);
}

{
  const immediate = useDebounceFn((value) => value.toUpperCase(), 50, { maxWait: 0 });
  check("debounce-zero-max-wait-bypasses-positive-duration", await Promise.all([immediate("one"), immediate("two")]), [
    "ONE",
    "TWO",
  ]);
}

{
  const invoked = [];
  const capped = useDebounceFn(
    (value) => {
      invoked.push(value);
      return value.toUpperCase();
    },
    50,
    { maxWait: 5 },
  );
  const first = capped("one");
  const latest = capped("two");
  check("debounce-max-wait-can-cancel-latest-result-while-invoking-callback", {
    values: (await Promise.all([first, latest])).map((value) => typeof value),
    invoked,
  }, { values: ["undefined", "undefined"], invoked: ["two"] });
}

{
  const leadingOnly = useThrottleFn((value) => value.toUpperCase(), 20);
  check("throttle-default-leading-only-reuses-earlier-result", await Promise.all([leadingOnly("one"), leadingOnly("two")]), [
    "ONE",
    "ONE",
  ]);
}

{
  const run = useDebounceFn((value) => value.toUpperCase(), 20);
  const first = run("one");
  await new Promise((resolve) => setTimeout(resolve, 80));
  const latest = run("two");
  check("debounce-await-longer-than-delay-preserves-first-result", await first, "ONE");
  check("debounce-await-longer-than-delay-latest-still-runs", await latest, "TWO");
}

{
  const trailing = useThrottleFn((value) => value.toUpperCase(), 20, true, true);
  const first = trailing("aa");
  const second = trailing("bb");
  const third = trailing("cc");
  check("throttle-trailing-three-calls-cancels-middle", {
    first: typeof (await first),
    second: typeof (await second),
    third: typeof (await third),
  }, { first: "string", second: "undefined", third: "string" });
}

{
  const noLeading = useThrottleFn((value) => value.toUpperCase(), 20, true, false);
  const first = noLeading("aa");
  const latest = noLeading("bb");
  check("throttle-no-leading-cancels-first", typeof (await first), "undefined");
  check("throttle-no-leading-latest-is-string", typeof (await latest), "string");
}

{
  const run = useDebounceFn((value) => value.toUpperCase(), 10);
  const first = run("one");
  run("two");
  const unhandled = await new Promise((resolve) => {
    const finish = (reason) => {
      process.off("unhandledRejection", finish);
      resolve(reason);
    };
    process.on("unhandledRejection", finish);
    first.then((value) => value.slice(0, 1));
    setTimeout(() => finish(null), 80);
  });
  check("debounce-then-handler-unhandled-rejection-is-typeerror", unhandled instanceof TypeError, true);
}

assert.equal(checks, 17, `expected 17 semantic assertions, got ${checks}`);
console.log(`filter-settlement oracle: ${checks} semantic assertions`);
