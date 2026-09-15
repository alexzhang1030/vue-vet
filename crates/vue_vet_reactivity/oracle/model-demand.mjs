/**
 * Vue 3.5.40 compiled-SFC premises for the two model-demand owners.
 *
 *   cd crates/vue_vet_reactivity/oracle && pnpm install --frozen-lockfile && node model-demand.mjs
 *
 * (a) Compiles every SFC under fixtures/projects/model-demand/ and the two
 *     rule fixture trees with @vue/compiler-sfc 3.5.40 (fail on error).
 * (b) Mounts the shipped parent/child pairs through createRenderer so
 *     onMounted runs, and asserts the throw / no-throw premises.
 */
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readdirSync, readFileSync, mkdirSync, writeFileSync, rmSync, realpathSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import path from "node:path";
import os from "node:os";

const oracleDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(oracleDir, "../../..");
const requireVue = createRequire(path.join(oracleDir, "package.json"));
const vue = requireVue("vue");
const sfc = requireVue("@vue/compiler-sfc");
const compilerPkg = requireVue("@vue/compiler-sfc/package.json");

assert.equal(vue.version, "3.5.40", `expected Vue 3.5.40, got ${vue.version}`);
assert.equal(compilerPkg.version, "3.5.40", `expected @vue/compiler-sfc 3.5.40, got ${compilerPkg.version}`);

const { createRenderer, h, nextTick, ref } = vue;
const vueIndex = realpathSync(requireVue.resolve("vue"));

function hostNode(kind, text = "") {
  return {
    kind,
    text,
    children: [],
    parent: null,
    props: {},
    get textContent() {
      return this.text + this.children.map((child) => child.textContent).join("");
    },
  };
}
function remove(node) {
  if (!node.parent) return;
  const index = node.parent.children.indexOf(node);
  if (index >= 0) node.parent.children.splice(index, 1);
  node.parent = null;
}
const renderer = createRenderer({
  createElement: (tag) => hostNode(tag),
  createText: (text) => hostNode("text", text),
  createComment: (text) => hostNode("comment", text),
  setText: (node, text) => {
    node.text = text;
  },
  setComment: (node, text) => {
    node.text = text;
  },
  setElementText: (node, text) => {
    for (const child of node.children) child.parent = null;
    node.children = [];
    node.text = text;
  },
  patchProp: (node, key, _previous, value) => {
    node.props[key] = value;
  },
  parentNode: (node) => node.parent,
  nextSibling: (node) => node.parent?.children[node.parent.children.indexOf(node) + 1] ?? null,
  querySelector: () => null,
  insert(node, parent, anchor) {
    remove(node);
    node.parent = parent;
    const index = anchor ? parent.children.indexOf(anchor) : -1;
    if (index < 0) parent.children.push(node);
    else parent.children.splice(index, 0, node);
  },
  remove,
});

function walkVueFiles(dir, acc = []) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === "node_modules") continue;
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walkVueFiles(full, acc);
    else if (entry.name.endsWith(".vue")) acc.push(full);
  }
  return acc;
}

const fixtureRoots = [
  path.join(repoRoot, "fixtures/projects/model-demand"),
  path.join(repoRoot, "fixtures/rules/no-model-default-unsynced-parent-demand"),
  path.join(repoRoot, "fixtures/rules/no-shared-default-cross-instance-demand"),
];

const compiled = [];
for (const root of fixtureRoots) {
  for (const file of walkVueFiles(root)) {
    const source = readFileSync(file, "utf8");
    const filename = path.relative(repoRoot, file);
    const parsed = sfc.parse(source, { filename });
    if (parsed.errors.length) {
      throw new Error(`parse ${filename}: ${parsed.errors.map((error) => error.message).join("; ")}`);
    }
    try {
      if (parsed.descriptor.scriptSetup || parsed.descriptor.script) {
        sfc.compileScript(parsed.descriptor, { id: filename.replace(/[^a-zA-Z0-9]/g, "_"), inlineTemplate: true });
      }
    } catch (error) {
      throw new Error(`compile ${filename}: ${String(error?.message ?? error).split("\n")[0]}`);
    }
    compiled.push(filename);
    console.log("compiles", filename);
  }
}
assert.ok(compiled.length > 0, "expected fixture SFCs");

function compileSfc(relPath, source, compiledRoot) {
  const filename = relPath;
  const id = relPath.replace(/[^a-zA-Z0-9]/g, "_");
  const parsed = sfc.parse(source, { filename });
  if (parsed.errors.length) {
    throw new Error(`parse ${relPath}: ${parsed.errors.map((error) => error.message).join("; ")}`);
  }
  const descriptor = parsed.descriptor;
  let code;
  if (descriptor.scriptSetup) {
    code = sfc.compileScript(descriptor, { id, inlineTemplate: true }).content;
  } else {
    const script = descriptor.script ? sfc.compileScript(descriptor, { id }) : null;
    const scriptCode = script ? sfc.rewriteDefault(script.content, "__sfc__") : "const __sfc__ = {}";
    const template = sfc.compileTemplate({
      source: descriptor.template.content,
      filename,
      id,
      compilerOptions: { bindingMetadata: script?.bindings },
    });
    if (template.errors.length) throw new Error(`template ${relPath}: ${template.errors.join("; ")}`);
    code = `${scriptCode}\n${template.code}\n__sfc__.render = render\nexport default __sfc__\n`;
  }
  code = code.replace(/from\s+(['"])vue\1/g, () => `from ${JSON.stringify(pathToFileURL(vueIndex).href)}`);
  code = code.replace(/from\s+(['"])(\.{1,2}\/[^'"]+)\.vue\1/g, (_m, q, p) => `from ${q}${p}.mts${q}`);
  const out = path.join(compiledRoot, relPath.replace(/\.vue$/, ".mts"));
  mkdirSync(path.dirname(out), { recursive: true });
  writeFileSync(out, code);
  return out;
}

async function mountFiles(files, rootRel) {
  const compiledRoot = path.join(os.tmpdir(), `vue-vet-model-demand-oracle-${Math.random().toString(16).slice(2)}`);
  rmSync(compiledRoot, { recursive: true, force: true });
  const compiledFiles = {};
  for (const [rel, source] of Object.entries(files)) {
    compiledFiles[rel] = compileSfc(rel, source, compiledRoot);
  }
  const Root = (await import(pathToFileURL(compiledFiles[rootRel]))).default;
  const errors = [];
  const originalWarn = console.warn;
  console.warn = () => {};
  const rootNode = hostNode("root");
  const app = renderer.createApp(Root);
  app.config.errorHandler = (error) => {
    errors.push(String(error?.message ?? error));
  };
  let mountError = null;
  try {
    app.mount(rootNode);
    await nextTick();
    await new Promise((resolve) => setTimeout(resolve, 0));
  } catch (error) {
    mountError = String(error?.message ?? error);
  } finally {
    console.warn = originalWarn;
    try {
      app.unmount();
    } catch {
      /* ignore */
    }
  }
  return { threw: errors.length > 0 || mountError !== null, errors, mountError };
}

async function sharedIdentity(childSource) {
  const compiledRoot = path.join(os.tmpdir(), `vue-vet-model-demand-id-${Math.random().toString(16).slice(2)}`);
  rmSync(compiledRoot, { recursive: true, force: true });
  const compiledChild = compileSfc("Child.vue", childSource, compiledRoot);
  const Child = (await import(pathToFileURL(compiledChild))).default;
  const left = ref(null);
  const right = ref(null);
  const probeApp = renderer.createApp({
    setup: () => () => h("main", [h(Child, { ref: left }), h(Child, { ref: right })]),
  });
  const originalWarn = console.warn;
  console.warn = () => {};
  try {
    probeApp.mount(hostNode("root"));
    const l = left.value?.model;
    const r = right.value?.model;
    return l === undefined || r === undefined ? "closed-instance" : l === r;
  } finally {
    console.warn = originalWarn;
    probeApp.unmount();
  }
}

function readFixture(...parts) {
  return readFileSync(path.join(repoRoot, ...parts), "utf8");
}

const childNumber = readFixture("fixtures/projects/model-demand/Child.vue");
const parentUndefined = readFixture("fixtures/projects/model-demand/Parent.vue");
const parentDefined = readFixture("fixtures/projects/model-demand/DefinedParent.vue");
const sharedChild = readFixture("fixtures/projects/model-demand/SharedChild.vue");
const sharedParent = readFixture("fixtures/projects/model-demand/SharedParent.vue");
const literalChild = readFixture("fixtures/projects/model-demand/LiteralChild.vue");
const literalParent = readFixture("fixtures/projects/model-demand/LiteralParent.vue");
const freshChild = readFixture("fixtures/projects/model-demand/FreshChild.vue");
const freshParent = readFixture("fixtures/projects/model-demand/FreshParent.vue");

const r1Throw = await mountFiles({ "Child.vue": childNumber, "Parent.vue": parentUndefined }, "Parent.vue");
assert.equal(r1Throw.threw, true, `rule 1 undefined parent must throw: ${JSON.stringify(r1Throw)}`);
console.log("runtime  rule1 undefined parent throws");

const r1Safe = await mountFiles({ "Child.vue": childNumber, "Parent.vue": parentDefined }, "Parent.vue");
assert.equal(r1Safe.threw, false, `rule 1 defined parent must not throw: ${JSON.stringify(r1Safe)}`);
console.log("runtime  rule1 defined parent is safe");

const r2Module = await mountFiles(
  { "SharedChild.vue": sharedChild, "SharedParent.vue": sharedParent },
  "SharedParent.vue",
);
assert.equal(r2Module.threw, true, `rule 2 module shared must throw: ${JSON.stringify(r2Module)}`);
assert.equal(await sharedIdentity(sharedChild), true, "module-script shared object must alias");
console.log("runtime  rule2 module-script shared throws and aliases");

const r2Literal = await mountFiles(
  { "LiteralChild.vue": literalChild, "LiteralParent.vue": literalParent },
  "LiteralParent.vue",
);
assert.equal(r2Literal.threw, true, `rule 2 literal default must throw: ${JSON.stringify(r2Literal)}`);
assert.equal(await sharedIdentity(literalChild), true, "literal object default must alias");
console.log("runtime  rule2 literal object default throws and aliases");

const r2Fresh = await mountFiles(
  { "FreshChild.vue": freshChild, "FreshParent.vue": freshParent },
  "FreshParent.vue",
);
assert.equal(r2Fresh.threw, false, `fresh factory must not throw: ${JSON.stringify(r2Fresh)}`);
assert.equal(await sharedIdentity(freshChild), false, "fresh factory must not alias");
console.log("runtime  rule2 fresh factory is isolated");

console.log(`ok ${compiled.length} fixtures compiled; rule 1/2 premises hold on Vue ${vue.version}`);
