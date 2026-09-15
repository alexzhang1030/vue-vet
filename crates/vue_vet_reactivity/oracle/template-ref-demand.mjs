/**
 * Vue 3.5.40 compiled SFC premises for template-ref demand rules.
 *
 * Locked oracle: this package's node_modules (Vue / compiler-sfc / compiler-dom 3.5.40).
 * Run: `just oracle-template-ref-demand`
 *
 * Browser layout/focus/selection claims stay outside this host. The renderer
 * only proves vnode patch and ref capability.
 */
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const oraclePkg = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "package.json");
const requireVue = createRequire(oraclePkg);
const vue = requireVue("vue");
const sfc = requireVue("@vue/compiler-sfc");
const compilerDom = requireVue("@vue/compiler-dom/package.json");
assert.equal(vue.version, "3.5.40", `expected Vue 3.5.40, got ${vue.version}`);
assert.equal(requireVue("@vue/compiler-sfc/package.json").version, "3.5.40");
assert.equal(compilerDom.version, "3.5.40");

const { createRenderer, nextTick } = vue;
const vueIndex = path.resolve(path.dirname(oraclePkg), "node_modules/vue/index.mjs");

function resultOf(fn) {
  try {
    return { value: fn() };
  } catch (error) {
    return { error: error.name };
  }
}

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
  nextSibling: (node) =>
    node.parent?.children[node.parent.children.indexOf(node) + 1] ?? null,
  insert(node, parent, anchor) {
    remove(node);
    node.parent = parent;
    const index = anchor ? parent.children.indexOf(anchor) : -1;
    if (index < 0) parent.children.push(node);
    else parent.children.splice(index, 0, node);
  },
  remove,
});

function mount(component) {
  const root = hostNode("root");
  const app = renderer.createApp(component);
  const exposed = app.mount(root);
  return { root, exposed, unmount: () => app.unmount() };
}

const compiledDir = mkdtempSync(path.join(tmpdir(), "vue-vet-template-ref-"));
async function compileComponent(name, source) {
  const parsed = sfc.parse(source, { filename: `${name}.vue` });
  assert.deepEqual(parsed.errors, []);
  const compiled = sfc.compileScript(parsed.descriptor, { id: name, inlineTemplate: true });
  const program = sfc.babelParse(compiled.content, { sourceType: "module" });
  const edits = program.program.body
    .filter((node) => node.type === "ImportDeclaration" && node.source.value === "vue")
    .map((node) => ({ start: node.source.start, end: node.source.end }))
    .sort((left, right) => right.start - left.start);
  let code = compiled.content;
  const vueUrl = JSON.stringify(pathToFileURL(vueIndex).href);
  for (const edit of edits) code = code.slice(0, edit.start) + vueUrl + code.slice(edit.end);
  const output = path.join(compiledDir, `${name}.mjs`);
  writeFileSync(output, code);
  return (await import(pathToFileURL(output).href)).default;
}

for (const phase of ["pre", "post"]) {
  const component = await compileComponent(
    `compiled-template-ref-${phase}`,
    `<script setup>
import { ref, watch } from 'vue'
const visible = ref(false)
const node = ref(null)
const observations = []
watch(visible, () => {
  try { observations.push(node.value.textContent) }
  catch (error) { observations.push(error.name) }
}, { flush: '${phase}' })
defineExpose({ visible, observations })
</script><template><span v-if="visible" ref="node">ready</span></template>`,
  );
  const app = mount(component);
  app.exposed.visible = true;
  await nextTick();
  assert.deepEqual(
    Array.from(app.exposed.observations),
    phase === "pre" ? ["TypeError"] : ["ready"],
    `conditional-template-ref-${phase}-demand`,
  );
  app.unmount();
}

{
  const component = await compileComponent(
    "compiled-previous-dom-snapshot",
    `<script setup>
import { ref, watch } from 'vue'
const count = ref(1)
const node = ref(null)
const observations = []
watch(count, () => observations.push(node.value.textContent), { flush: 'pre' })
defineExpose({ count, observations })
</script><template><div ref="node">{{ count }}</div></template>`,
  );
  const app = mount(component);
  app.exposed.count = 2;
  await nextTick();
  assert.deepEqual(Array.from(app.exposed.observations), ["1"], "previous host snapshot");
  app.unmount();
}

for (const [name, memo] of [
  ["compiled-memo-ref-missing-source", "[revision]"],
  ["compiled-memo-ref-complete-source", "[revision, visible]"],
]) {
  const component = await compileComponent(
    name,
    `<script setup>
import { ref } from 'vue'
const revision = ref(0)
const visible = ref(false)
const node = ref(null)
function inspect() { return node.value.textContent }
defineExpose({ revision, visible, inspect })
</script><template><div v-memo="${memo}"><span v-if="visible" ref="node">ready</span></div></template>`,
  );
  const app = mount(component);
  app.exposed.visible = true;
  await nextTick();
  const afterTick = resultOf(() => app.exposed.inspect());
  if (memo.includes("visible")) {
    assert.deepEqual(afterTick, { value: "ready" }, `${name}-after-tick-ref-demand`);
  } else {
    assert.deepEqual(afterTick, { error: "TypeError" }, `${name}-after-tick-ref-demand`);
  }
  app.exposed.revision++;
  await nextTick();
  assert.equal(app.exposed.inspect(), "ready", `${name}-explicit-invalidation-repairs-ref-demand`);
  app.unmount();
}

{
  const component = await compileComponent(
    "compiled-empty-memo-ref",
    `<script setup>
import { ref } from 'vue'
const visible = ref(false)
const node = ref(null)
function inspect() { return node.value.textContent }
defineExpose({ visible, inspect })
</script><template><div v-memo="[]"><span v-if="visible" ref="node">ready</span></div></template>`,
  );
  const app = mount(component);
  app.exposed.visible = true;
  await nextTick();
  assert.deepEqual(resultOf(() => app.exposed.inspect()), { error: "TypeError" }, "empty-memo-failed-creation");
  app.unmount();
}

console.log(
  JSON.stringify({
    vue: vue.version,
    compilerSfc: requireVue("@vue/compiler-sfc/package.json").version,
    compilerDom: compilerDom.version,
    ok: true,
  }),
);
