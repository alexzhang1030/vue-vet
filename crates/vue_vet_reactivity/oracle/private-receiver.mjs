/**
 * Vue 3.5.40 runtime premises for
 * `vue-vet/reactivity/no-reactive-private-field-access`.
 *
 * Locked oracle: this package's node_modules (Vue 3.5.40).
 * Run: `just oracle-private-receiver`
 */
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import path from "node:path";

const oraclePkg = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "package.json");
const requireVue = createRequire(oraclePkg);
const vue = requireVue("vue");
assert.equal(vue.version, "3.5.40", `expected Vue 3.5.40, got ${vue.version}`);

const { reactive, readonly, shallowReactive, shallowReadonly, toRaw } = vue;

function throwsTypeError(fn, name) {
  let failed = false;
  try {
    fn();
  } catch (error) {
    failed = true;
    assert.equal(error instanceof TypeError, true, `${name} must be TypeError, got ${error}`);
  }
  assert.equal(failed, true, `${name} must throw`);
}

function doesNotThrow(fn, name) {
  try {
    fn();
  } catch (error) {
    assert.fail(`${name} must not throw, got ${error}`);
  }
}

{
  class Counter {
    #n = 1;
    read() {
      return this.#n;
    }
    get value() {
      return this.#n;
    }
  }
  throwsTypeError(() => reactive(new Counter()).read(), "reactive proxy method this.#n");
  throwsTypeError(() => reactive(new Counter()).value, "reactive proxy getter this.#n");
  throwsTypeError(() => shallowReactive(new Counter()).read(), "shallowReactive proxy this.#n");
  throwsTypeError(() => readonly(new Counter()).read(), "readonly proxy this.#n");
  throwsTypeError(() => shallowReadonly(new Counter()).read(), "shallowReadonly proxy this.#n");
}

{
  class Skip {
    #n = 1;
    __v_skip = true;
    read() {
      return this.#n;
    }
  }
  class RawFlag {
    #n = 1;
    __v_raw = {};
    read() {
      return this.#n;
    }
  }
  class TagField {
    #n = 1;
    [Symbol.toStringTag] = "Counter";
    read() {
      return this.#n;
    }
  }
  class TagGetter {
    #n = 1;
    get [Symbol.toStringTag]() {
      return "Counter";
    }
    read() {
      return this.#n;
    }
  }
  const skip = new Skip();
  assert.equal(reactive(skip), skip, "reactive() is a no-op when __v_skip is set");
  doesNotThrow(() => reactive(new Skip()).read(), "__v_skip field keeps the brand");
  const rawFlag = new RawFlag();
  assert.equal(reactive(rawFlag), rawFlag, "reactive() is a no-op when __v_raw is set");
  doesNotThrow(() => reactive(new RawFlag()).read(), "__v_raw field keeps the brand");
  const tagged = new TagField();
  assert.equal(reactive(tagged), tagged, "reactive() is a no-op for [Symbol.toStringTag] field");
  doesNotThrow(() => reactive(new TagField()).read(), "[Symbol.toStringTag] field keeps the brand");
  const taggedGet = new TagGetter();
  assert.equal(reactive(taggedGet), taggedGet, "reactive() is a no-op for [Symbol.toStringTag] getter");
  doesNotThrow(() => reactive(new TagGetter()).read(), "[Symbol.toStringTag] getter keeps the brand");
}

{
  class Shadowed {
    #n = 1;
    read = function () {
      return 1;
    };
    read() {
      return this.#n;
    }
  }
  class Replaced {
    #n = 1;
    patched = (this.read = () => 0);
    read() {
      return this.#n;
    }
  }
  doesNotThrow(() => reactive(new Shadowed()).read(), "own field shadows prototype method");
  doesNotThrow(() => reactive(new Replaced()).read(), "field initializer replaces method");
}

{
  class CtorItems {
    #n = 1;
    constructor() {
      this.items = [];
    }
    read() {
      return this.#n;
    }
  }
  class CtorPrivate {
    #n;
    constructor() {
      this.#n = 5;
    }
    read() {
      return this.#n;
    }
  }
  throwsTypeError(() => reactive(new CtorItems()).read(), "constructor this.items = [] still throws");
  throwsTypeError(() => reactive(new CtorPrivate()).read(), "constructor this.#n = 5 still throws");
}

{
  class InArg {
    #n = 1;
    read() {
      return String(this.#n);
    }
  }
  throwsTypeError(() => reactive(new InArg()).read(), "String(this.#n) still throws");
}

{
  class AsyncCounter {
    #n = 1;
    async read() {
      return this.#n;
    }
  }
  let rejected = null;
  const pending = reactive(new AsyncCounter()).read();
  await pending.then(
    () => {
      assert.fail("async method must not fulfill");
    },
    (error) => {
      rejected = error;
    },
  );
  assert.equal(rejected instanceof TypeError, true, "async method rejects with TypeError");
}

{
  let threw = null;
  try {
    const proxy = reactive(new Later());
    void proxy.read();
    class Later {
      #n = 1;
      read() {
        return this.#n;
      }
    }
  } catch (error) {
    threw = error;
  }
  assert.equal(threw instanceof ReferenceError, true, "TDZ new Class() is ReferenceError");
}

{
  class Bound {
    #n = 1;
    constructor() {
      this.read = this.read.bind(this);
    }
    read() {
      return this.#n;
    }
  }
  class Arrow {
    #n = 2;
    read = () => this.#n;
  }
  doesNotThrow(() => reactive(new Bound()).read(), "constructor-bound method keeps raw this");
  doesNotThrow(() => reactive(new Arrow()).read(), "arrow field keeps lexical this");
  const raw = new Bound();
  assert.equal(toRaw(reactive(raw)), raw, "toRaw unwraps the proxy");
}

console.log("private-receiver oracle: ok (Vue 3.5.40)");
