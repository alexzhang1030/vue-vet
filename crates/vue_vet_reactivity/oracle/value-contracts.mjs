/**
 * Vue 3.5.40 runtime premises for value-contract rules (issue #224 batch 3).
 *
 * Locked oracle: this package's node_modules (Vue 3.5.40).
 * Run: `just oracle-value-contracts`
 */
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import path from "node:path";

const oraclePkg = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "package.json");
const requireVue = createRequire(oraclePkg);
const vue = requireVue("vue");
assert.equal(vue.version, "3.5.40", `expected Vue 3.5.40, got ${vue.version}`);

const { customRef, effectScope, reactive, ref, toRef, toRefs, watch } = vue;

function throws(fn, name) {
  let failed = false;
  try {
    fn();
  } catch (error) {
    failed = true;
    assert.equal(error instanceof TypeError, true, `${name} must be TypeError, got ${error}`);
  }
  assert.equal(failed, true, `${name} must throw`);
}

// customRef: missing get + read throws; missing set + write throws.
{
  const missingGet = customRef(() => ({
    set(value) {
      void value;
    },
  }));
  throws(() => missingGet.value, "customRef without get must throw on read");

  const missingSet = customRef(() => ({
    get() {
      return 1;
    },
  }));
  throws(() => {
    missingSet.value = 2;
  }, "customRef without set must throw on write");

  const noncallableGet = customRef(() => ({
    get: 1,
    set() {},
  }));
  throws(() => noncallableGet.value, "customRef with noncallable get must throw on read");

  const noncallableSet = customRef(() => ({
    get() {
      return 1;
    },
    set: null,
  }));
  throws(() => {
    noncallableSet.value = 2;
  }, "customRef with null set must throw on write");

  const unused = customRef(() => ({
    set() {},
  }));
  assert.equal(typeof unused, "object", "unused invalid customRef must still construct");

  const getterOnly = customRef(() => ({
    get() {
      return 7;
    },
  }));
  assert.equal(getterOnly.value, 7, "getter-only customRef must read");

  let stored = 0;
  const both = customRef((track, trigger) => ({
    get() {
      track();
      return stored;
    },
    set(value) {
      stored = value;
      trigger();
    },
  }));
  both.value = 4;
  assert.equal(both.value, 4, "valid get/set customRef must round-trip");
}

// effectScope.stop then run returns undefined; demanded member throws.
{
  const live = effectScope();
  const liveResult = live.run(() => ({ count: 1 }));
  assert.equal(liveResult.count, 1, "live scope.run must return the callback result");

  const stopped = effectScope();
  stopped.stop();
  const result = stopped.run(() => ({ count: 1 }));
  assert.equal(result, undefined, "stopped scope.run must return undefined");
  throws(() => result.count, "using stopped scope.run result as object must throw");

  const ignored = effectScope();
  ignored.stop();
  ignored.run(() => ({ count: 1 }));

  const returned = effectScope();
  const handle = returned.run(() => returned);
  assert.equal(handle, returned, "returning the live scope handle is legitimate");
  returned.stop();
}

// toRefs missing key is undefined; .value throws. Known key is safe. toRef is separate.
{
  const bag = toRefs(reactive({ count: 1 }));
  assert.equal(bag.count.value, 1, "known toRefs key must unwrap");
  assert.equal(bag.missing, undefined, "missing toRefs member is undefined");
  throws(() => bag.missing.value, "missing toRefs key .value must throw");

  const { missing } = toRefs(reactive({ count: 1 }));
  assert.equal(missing, undefined, "destructured missing toRefs key is undefined");
  throws(() => missing.value, "destructured missing key .value must throw");

  const direct = toRefs(reactive({ count: 1 })).missing;
  throws(() => direct.value, "direct missing key .value must throw");

  const future = toRef(reactive({ count: 1 }), "future");
  assert.equal(future.value, undefined, "toRef on a future key is a valid property ref");
  future.value = 9;
  assert.equal(future.value, 9, "toRef future key must write through");
}

{
  function make(undefined) {
    const count = customRef(() => ({ get: undefined }));
    return count.value;
  }
  assert.equal(make(() => 7), 7, "resolved local undefined getter must read");
  let out;
  out = customRef(() => ({
    get() {
      return 7;
    },
  })).value;
  assert.equal(out, 7, "assignment RHS .value is a getter read");
  const deleted = customRef(() => ({ set() {} }));
  assert.equal(delete deleted.value, true, "delete .value is not a getter call");
  const mutated = customRef(() => ({ set() {} }));
  mutated._get = () => 7;
  assert.equal(mutated.value, 7, "installed getter after mutation must read");
}

{
  const logical = effectScope();
  false && logical.stop();
  assert.equal(logical.run(() => ({ count: 1 })).count, 1, "logical stop must leave the scope live");
  logical.stop();
  const method = effectScope();
  const realStop = method.stop.bind(method);
  method.stop = () => {};
  method.stop();
  assert.equal(method.run(() => ({ count: 1 })).count, 1, "replaced stop must leave the scope live");
  realStop();
  const guarded = effectScope();
  guarded.stop();
  const guardedResult = guarded.run(() => ({ count: 1 }));
  let consumed = false;
  if (guardedResult) {
    void guardedResult.count;
    consumed = true;
  }
  assert.equal(consumed, false, "truthy guard must skip undefined result");
  const shorted = effectScope();
  shorted.stop();
  false && shorted.run(() => ({ count: 1 })).count;
}

{
  const mutated = toRefs(reactive({ count: 1 }));
  mutated.missing = ref(2);
  assert.equal(mutated.missing.value, 2, "assigned missing bag key must read");
  let { missing } = toRefs(reactive({ count: 1 }));
  missing = ref(2);
  assert.equal(missing.value, 2, "reassigned destructure must read");
  assert.equal(
    toRefs(reactive({ __proto__: { inherited: 1 }, count: 1 })).inherited.value,
    1,
    "custom prototype source keys must be enumerated",
  );
  assert.equal(
    toRefs(reactive({ count: 1 })).toString.value,
    undefined,
    "inherited bag function .value is undefined without throw",
  );
  const refs = toRefs(reactive({ count: 1 }));
  let consumed = false;
  if (refs.missing) {
    void refs.missing.value;
    consumed = true;
  }
  assert.equal(consumed, false, "missing-key guard must skip dereference");
}

{
  const state = reactive({ child: { count: 1 } });
  let runs = 0;
  false && watch(state.child, () => { runs += 1; }, { flush: "sync" });
  state.child = { count: 2 };
  assert.equal(runs, 0, "short-circuit watch must not subscribe");
}

{
  let consumers = 0;
  function guardedScope() {
    const scope = effectScope();
    scope.stop();
    const result = scope.run(() => ({ count: 1 }));
    if (!result) return;
    consumers += 1;
    void result.count;
  }
  function guardedBag() {
    const bag = toRefs(reactive({ count: 1 }));
    if (!bag.missing) return;
    consumers += 1;
    void bag.missing.value;
  }
  guardedScope();
  guardedBag();
  assert.equal(consumers, 0, "prior return after failed demand must skip consumers");

  const installed = customRef(() => ({
    get() {
      this._set = (value) => {
        this.saved = value;
      };
      return 1;
    },
  }));
  assert.equal(installed.value, 1, "getter may install a setter on the impl receiver");
  installed.value = 7;
  assert.equal(installed.saved, 7, "later write uses the getter-installed setter");

  const closed = customRef(() => ({
    get() {
      return 1;
    },
  }));
  assert.equal(closed.value, 1, "closed getter still reads");
  throws(() => {
    closed.value = 7;
  }, "closed getter does not install a setter");

  function install(target) {
    Object.assign(target, { added: 7 });
  }
  const helperState = reactive({ initial: 1 });
  install(helperState);
  assert.equal(toRefs(helperState).added.value, 7, "helper mutation is visible to toRefs bag");
  const helperChain = reactive({ initial: 1 });
  install(helperChain);
  assert.equal(toRefs(helperChain).added.value, 7, "helper mutation is visible to chained toRefs");
  const helperDestructure = reactive({ initial: 1 });
  install(helperDestructure);
  const { added } = toRefs(helperDestructure);
  assert.equal(added.value, 7, "helper mutation is visible to destructured toRefs");

  let computedSaved = 0;
  const computedKey = customRef(() => ({
    get() {
      const marker = { [this._set = (value) => { computedSaved = value; }]: 1 };
      return marker;
    },
  }));
  void computedKey.value;
  computedKey.value = 7;
  assert.equal(computedSaved, 7, "computed object key may install a setter on the impl receiver");
}

console.log("value-contracts oracle: all Vue 3.5.40 premises passed");
