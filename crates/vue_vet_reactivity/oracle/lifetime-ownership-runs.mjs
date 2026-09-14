/**
 * Vue 3.5.40 nested-watch / detached-scope / returned-handle evidence.
 *
 *   node lifetime-ownership-runs.mjs
 *
 * Residual callback counts are bounded hits, not infinite-execution claims.
 * `dormantComputedRetention` keeps callback count 0 with two retained
 * subscribers (`inner.dep.sc === 2`) at this Vue pin. Outer stable-result
 * getters / constant computed sources stay at one callback; inner assignment
 * RHS and ignored effect-family `once` retain two subscribers.
 */
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { readFileSync } from "node:fs";

const requireVue = createRequire(fileURLToPath(new URL("./package.json", import.meta.url)));
const vue = requireVue("vue");
assert.equal(vue.version, "3.5.40", `expected Vue 3.5.40, got ${vue.version}`);

const casesUrl = new URL("./lifetime-ownership-cases.mjs", import.meta.url);
const source = readFileSync(casesUrl, "utf8");
const vueUrl = new URL("./node_modules/vue/index.mjs", import.meta.url);
const moduleSource = source.replace("from 'vue'", `from '${vueUrl.href}'`);
const cases = await import(`data:text/javascript;base64,${Buffer.from(moduleSource).toString("base64")}`);

const residualTwo = new Set([
  "inheritedOptionsRemainRepeatable",
  "assignmentRhsTrackedSource",
  "effectFamilyOnceIgnored",
  "getterBeforeStop",
  "assignmentDefaultRead",
  "assignmentComputedKeyRead",
  "wrappedCustomRefGetter",
]);
const retainedTwo = new Set([
  "dormantComputedRetention",
  "returnedTrackedSource",
  "assignmentRhsTrackedSource",
  "effectFamilyOnceIgnored",
  "getterBeforeStop",
  "assignmentDefaultRead",
  "assignmentComputedKeyRead",
  "wrappedCustomRefGetter",
]);
const results = {};
for (const [name, run] of Object.entries(cases)) {
  results[name] = await run();
  const expectedHits = name === "lateCurrentScopeKeepsOrphanOwner"
    ? 1
    : name.startsWith("positive") || residualTwo.has(name)
      ? 2
      : 0;
  assert.equal(results[name].residualHits, expectedHits, name);
}
assert.equal(results.dormantComputedRetention.retainedSubscriptions, 2);
for (const name of retainedTwo) {
  assert.equal(results[name].retainedSubscriptions, 2, name);
}
assert.equal(results.stableOuterGetter.outerRuns, 1);
assert.equal(results.dormantComputedOuter.outerRuns, 1);
assert.equal(results.currentScopeOwner.ownedScopes, 2);
assert.equal(results.safeComputedGetterOuter.outerRuns, 1);
assert.equal(results.safeComputedGetterOuter.retainedSubscriptions, 0);
assert.equal(results.lateCurrentScopeKeepsOrphanOwner.currentScopeMissing, true);
assert.equal(results.lateCurrentScopeKeepsOrphanOwner.retainedSubscriptions, 1);
assert.equal(results.deadGetterRead.retainedSubscriptions, 0);
assert.equal(results.deleteEffectProperty.retainedSubscriptions, 0);
assert.equal(results.objectDefaultSkipped.snapshot, 42);
assert.equal(results.arrayDefaultSkipped.snapshot, 42);
assert.equal(results.asyncGetterAfterAwait.retainedSubscriptions, 0);
assert.equal(results.conditionalCurrentScope.retainedSubscriptions, 0);
assert.equal(results.cyclicComputedTerminates.leftUndefined, true);
assert.equal(results.cyclicComputedTerminates.rightUndefined, true);

console.log(JSON.stringify({ vue: vue.version, results }, null, 2));
