import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { parse, compileScript } from '@vue/compiler-sfc'
import { compileSfc } from './lib/compile-sfc.mjs'

const root = dirname(fileURLToPath(import.meta.url))
const outDir = join(root, 'output/compile')
const genDir = join(outDir, 'generated')
const expected = JSON.parse(readFileSync(join(root, 'expectations/counts.json'), 'utf8'))
const observation35 = JSON.parse(
  readFileSync(join(root, 'expectations/compiler-sfc-3.5.42.json'), 'utf8'),
)

rmSync(outDir, { recursive: true, force: true })
mkdirSync(genDir, { recursive: true })

const sources = [
  {
    id: 'script-setup-vapor-attr',
    source: `<script setup vapor>
import { ref } from 'vue'
const n = ref(0)
</script>
<template>
  <button @click="n++">{{ n }}</button>
</template>`,
  },
  {
    id: 'template-vapor-attr-with-setup',
    source: `<script setup>
import { ref } from 'vue'
const n = ref(0)
</script>
<template vapor>
  <p>{{ n }}</p>
</template>`,
  },
  {
    id: 'script-vapor-is-setup',
    source: `<script vapor>
import { ref } from 'vue'
const n = ref(1)
</script>
<template>
  <span>{{ n }}</span>
</template>`,
  },
  {
    id: 'control-script-setup-no-marker',
    source: `<script setup>
import { ref } from 'vue'
const n = ref(0)
</script>
<template>
  <button @click="n++">{{ n }}</button>
</template>`,
  },
  {
    id: 'options-only',
    source: `<script>
export default {
  data() { return { n: 0 } }
}
</script>
<template>
  <p>{{ n }}</p>
</template>`,
  },
  {
    id: 'ordinary-script-imports-only',
    source: `<script>
import { ref } from 'vue'
export const n = ref(0)
</script>
<template>
  <p>static</p>
</template>`,
  },
  {
    id: 'template-only',
    source: `<template>
  <p>hello</p>
</template>`,
  },
  {
    id: 'template-only-vapor-attr',
    source: `<template vapor>
  <p>hello</p>
</template>`,
  },
  {
    id: 'template-vapor-plus-options-script',
    source: `<script>
export default {
  data() { return { n: 0 } }
}
</script>
<template vapor>
  <p>{{ n }}</p>
</template>`,
  },
  {
    id: 'dual-script-setup-vapor',
    source: `<script>
export default { name: 'Counter' }
</script>
<script setup vapor>
import { ref } from 'vue'
const count = ref(0)
</script>
<template>
  <button @click="count++">{{ count }}</button>
</template>`,
  },
  {
    id: 'dual-script-no-marker',
    source: `<script>
export default { name: 'Counter' }
</script>
<script setup>
import { ref } from 'vue'
const count = ref(0)
</script>
<template>
  <button @click="count++">{{ count }}</button>
</template>`,
  },
  {
    id: 'script-vapor-export-default',
    source: `<script vapor>
import { h } from 'vue'
export default {
  setup() {
    return () => h('div', 'hi')
  }
}
</script>`,
  },
  {
    id: 'v-if-v-for-v-show',
    source: `<script setup vapor>
const items = [1, 2]
const ok = true
</script>
<template>
  <ul v-if="ok">
    <li v-for="item in items" :key="item">{{ item }}</li>
  </ul>
  <p v-show="ok">shown</p>
</template>`,
  },
  {
    id: 'v-html-v-text-v-once',
    source: `<script setup vapor>
const html = '<b>x</b>'
</script>
<template>
  <div v-html="html"></div>
  <span v-text="html"></span>
  <p v-once>{{ html }}</p>
</template>`,
  },
  {
    id: 'v-memo',
    source: `<script setup vapor>
import { ref } from 'vue'
const memoKey = ref(0)
const shown = ref(0)
</script>
<template>
  <div v-memo="[memoKey]">{{ shown }}</div>
</template>`,
  },
  {
    id: 'teleport-keep-alive-transition-suspense',
    source: `<script setup vapor>
import { ref } from 'vue'
const show = ref(true)
</script>
<template>
  <Teleport to="body"><span>t</span></Teleport>
  <KeepAlive><Comp v-if="show" /></KeepAlive>
  <Transition><p v-if="show">x</p></Transition>
  <Suspense><Comp /></Suspense>
</template>`,
  },
]

const modes = [
  {
    name: 'default-inline',
    featuresVapor: false,
    ssr: false,
    inlineTemplate: true,
    isProd: true,
  },
  {
    name: 'plugin-force-inline',
    featuresVapor: true,
    ssr: false,
    inlineTemplate: true,
    isProd: true,
  },
  {
    name: 'naive-compileScript-vapor-inline',
    featuresVapor: false,
    ssr: false,
    inlineTemplate: true,
    isProd: true,
    naiveCompileScriptVapor: true,
  },
  {
    name: 'default-split',
    featuresVapor: false,
    ssr: false,
    inlineTemplate: false,
    isProd: true,
  },
]

const ssrMode = {
  name: 'ssr-inline',
  featuresVapor: false,
  ssr: true,
  inlineTemplate: true,
  isProd: true,
}

function rowId(sourceId, modeName) {
  return `${sourceId}__${modeName}`
}

function writeGenerated(id, kind, content) {
  if (!content) return null
  const rel = `generated/${id}.${kind}.js`
  writeFileSync(join(outDir, rel), content)
  return rel
}

const rows = []
for (const src of sources) {
  const filename = `${src.id}.vue`
  for (const mode of modes) {
    const compiled = compileSfc(src.source, { filename, ...mode })
    const id = rowId(src.id, mode.name)
    compiled.rowId = id
    compiled.sourceId = src.id
    compiled.mode = mode.name
    compiled.scriptPath = writeGenerated(id, 'script', compiled.script?.content)
    compiled.templatePath = writeGenerated(id, 'template', compiled.template?.code)
    compiled.vaporDirectPath = writeGenerated(id, 'vapor-direct', compiled.vaporDirect?.code)
    rows.push(compiled)
  }
}

for (const srcId of ['script-setup-vapor-attr', 'control-script-setup-no-marker']) {
  const src = sources.find(s => s.id === srcId)
  const compiled = compileSfc(src.source, { filename: `${src.id}.vue`, ...ssrMode })
  const id = rowId(src.id, ssrMode.name)
  compiled.rowId = id
  compiled.sourceId = src.id
  compiled.mode = ssrMode.name
  compiled.scriptPath = writeGenerated(id, 'script', compiled.script?.content)
  compiled.templatePath = writeGenerated(id, 'template', compiled.template?.code)
  rows.push(compiled)
}

const byId = Object.fromEntries(rows.map(r => [r.rowId, r]))

const assertions = []
let assertionFailed = 0

function assert(name, cond, detail) {
  const ok = !!cond
  if (!ok) assertionFailed += 1
  assertions.push({ name, ok, detail: ok ? undefined : detail ?? null })
}

function row(id) {
  const r = byId[id]
  assert(`row-exists:${id}`, !!r, 'missing compile row')
  return r
}

const setupVapor = row('script-setup-vapor-attr__default-inline')
assert('opt-in:script-setup-vapor descriptor.vapor', setupVapor?.descriptor.vapor === true)
assert('opt-in:script-setup-vapor plugin eligible', setupVapor?.pluginForceEligible === true)
assert('opt-in:script-setup-vapor plugin vapor without force', setupVapor?.pluginVapor === true)
assert('opt-in:script-setup-vapor script stage ok', setupVapor?.stages.script.present && setupVapor?.stages.script.ok)
assert('opt-in:script-setup-vapor __vapor flag', setupVapor?.script?.vaporFlagInCode === true)
assert('opt-in:script-setup-vapor no openBlock', setupVapor?.script?.usesOpenBlock === false)

const controlDefault = row('control-script-setup-no-marker__default-inline')
assert('force:setup-no-marker eligible', controlDefault?.pluginForceEligible === true)
assert('force:setup-no-marker default stays VDOM', controlDefault?.pluginVapor === false)
assert('force:setup-no-marker default openBlock', controlDefault?.script?.usesOpenBlock === true)
assert('force:setup-no-marker default no __vapor', controlDefault?.script?.vaporFlagInCode === false)

const controlForce = row('control-script-setup-no-marker__plugin-force-inline')
assert('force:setup-no-marker force vapor', controlForce?.pluginVapor === true)
assert('force:setup-no-marker force __vapor', controlForce?.script?.vaporFlagInCode === true)
assert('force:setup-no-marker force no openBlock', controlForce?.script?.usesOpenBlock === false)

const optionsForce = row('options-only__plugin-force-inline')
assert('force:options ineligible', optionsForce?.pluginForceEligible === false)
assert('force:options stays non-vapor', optionsForce?.pluginVapor === false)
assert('force:options no __vapor', optionsForce?.script?.vaporFlagInCode === false)
assert('force:options script stage present', optionsForce?.stages.script.present === true)

const optionsNaive = row('options-only__naive-compileScript-vapor-inline')
assert(
  'compiler-api:options naive path labeled',
  optionsNaive?.vaporPath === 'compiler-sfc-api-vapor-true',
)
assert('compiler-api:options naive effectiveVapor', optionsNaive?.effectiveVapor === true)
assert(
  'compiler-api:options naive still no __vapor',
  optionsNaive?.script?.ok === true && optionsNaive?.script?.vaporFlagInCode === false,
)

const importsForce = row('ordinary-script-imports-only__plugin-force-inline')
assert('force:imports-only ineligible', importsForce?.pluginForceEligible === false)
assert('force:imports-only no __vapor', importsForce?.script?.vaporFlagInCode === false)

const templateDefault = row('template-only__default-inline')
assert('template-only:script stage absent', templateDefault?.stages.script.present === false)
assert('template-only:script not invoked', templateDefault?.stages.script.invoked === false)
assert('template-only:script ok is null', templateDefault?.stages.script.ok === null)
assert('template-only:plugin eligible', templateDefault?.pluginForceEligible === true)
assert('template-only:default pluginVapor false', templateDefault?.pluginVapor === false)
assert('template-only:template stage ok', templateDefault?.stages.template.ok === true)
assert(
  'template-only:default plugin seed empty',
  templateDefault?.composedComponent?.kind === 'plugin-pipeline-emulation' &&
    Object.keys(templateDefault.composedComponent.object || {}).length === 0,
)

const templateForce = row('template-only__plugin-force-inline')
assert('template-only:force pluginVapor', templateForce?.pluginVapor === true)
assert('template-only:force script still absent', templateForce?.stages.script.present === false)
assert('template-only:force template vapor helper', templateForce?.template?.usesVaporTemplateHelper === true)
assert(
  'template-only:force plugin seed __vapor',
  templateForce?.composedComponent?.object?.__vapor === true,
)

const templateAttr = row('template-only-vapor-attr__default-inline')
assert('template-only-attr:descriptor.vapor', templateAttr?.descriptor.vapor === true)
assert('template-only-attr:plugin vapor', templateAttr?.pluginVapor === true)
assert('template-only-attr:script absent', templateAttr?.stages.script.present === false)

const hybrid = row('template-vapor-plus-options-script__default-inline')
assert('hybrid:force ineligible', hybrid?.pluginForceEligible === false)
assert('hybrid:descriptor.vapor wins', hybrid?.pluginVapor === true && hybrid?.descriptor.vapor === true)
assert('hybrid:options script ok without __vapor', hybrid?.script?.ok === true && hybrid?.script?.vaporFlagInCode === false)
assert('hybrid:vapor template IR', hybrid?.vaporDirect?.ok === true)

const dual = row('dual-script-setup-vapor__default-inline')
assert('dual-script:__vapor', dual?.script?.vaporFlagInCode === true)
assert('dual-script:Object.assign merge', dual?.script?.usesObjectAssign === true)
assert(
  'dual-script:name preserved',
  typeof dual?.script?.content === 'string' && dual.script.content.includes("name: 'Counter'"),
)

const exportDefault = row('script-vapor-export-default__default-inline')
assert('setup-export:script stage fails', exportDefault?.stages.script.ok === false)
assert(
  'setup-export:compiler message',
  typeof exportDefault?.stages.script.error === 'string' &&
    exportDefault.stages.script.error.includes('cannot contain ES module exports'),
)
assert('setup-export:parse separate from script', exportDefault?.stages.parse.ok === true)

const memo = row('v-memo__default-inline')
assert('memo:compiles', memo?.script?.ok === true && memo?.script?.vaporFlagInCode === true)
assert('memo:no memo helper', memo?.script?.hasMemoHelper === false)
assert('memo:effect reads shown', memo?.script?.renderEffectIncludesShown === true)
assert('memo:effect omits memoKey', memo?.script?.renderEffectIncludesMemoKey === false)
assert(
  'memo:helpers omit memo',
  Array.isArray(memo?.vaporDirect?.helpers) && !memo.vaporDirect.helpers.some(h => /memo/i.test(h)),
)

const onceSource = `<script setup>
import { ref } from 'vue'
const n = ref(0)
</script>
<template>
  <div>
    <span class="once" v-once>{{ n }}</span>
    <span class="live">{{ n }}</span>
  </div>
</template>`
const onceDescriptor = parse(onceSource, { filename: 'v-once-ref.vue' })
assert('once-probe:parse ok', onceDescriptor.errors.length === 0)
try {
  const onceCompiled = compileScript(onceDescriptor.descriptor, {
    id: 'data-v-vonceref',
    vapor: true,
    inlineTemplate: true,
    isProd: true,
  })
  writeGenerated('v-once-ref-probe', 'script', onceCompiled.content)
  const onceContent = onceCompiled.content
  const effectCount = [...onceContent.matchAll(/_renderEffect\(/g)].length
  assert('once-probe:__vapor', onceContent.includes('__vapor: true'))
  assert('once-probe:one renderEffect for live text', effectCount === 1, `effects=${effectCount}`)
  assert(
    'once-probe:once text is one-shot',
    onceContent.includes('_setText(x0, _toDisplayString(n.value))') &&
      !/_renderEffect\([^)]*x0/.test(onceContent),
    'once node was wrapped in renderEffect',
  )
  assert(
    'once-probe:live text is an effect',
    /_renderEffect\(\(\) => _setText\(x1, _toDisplayString\(n\.value\)\)\)/.test(onceContent),
  )
} catch (e) {
  assert('once-probe:compile', false, e.message)
}

const ssr = row('script-setup-vapor-attr__ssr-inline')
assert('ssr:script ok', ssr?.script?.ok === true)
assert('ssr:stage marked', ssr?.stages.ssr.used === true)
assert('ssr:__vapor retained', ssr?.script?.content?.includes('__vapor: true') === true)
assert('ssr:ssrInlineRender', ssr?.script?.content?.includes('__ssrInlineRender') === true)
assert(
  'ssr:server-renderer interpolate',
  ssr?.script?.content?.includes('ssrInterpolate') === true &&
    ssr?.script?.content?.includes('vue/server-renderer') === true,
)

const sfcPkg = JSON.parse(readFileSync(join(root, 'node_modules/@vue/compiler-sfc/package.json'), 'utf8'))
assert('tuple:compiler-sfc version', sfcPkg.version === expected.auditedTuple.compilerSfc)
assert(
  'tuple:compiler-sfc depends on compiler-vapor',
  sfcPkg.dependencies?.['@vue/compiler-vapor'] === expected.auditedTuple.compilerVapor,
)
assert('observation:3.5.42 blocked', observation35.verdict === 'blocked' && observation35.vaporContentHits === 0)

assert(
  'matrix:source count',
  sources.length === expected.compile.sourceFixtures,
  `${sources.length} != ${expected.compile.sourceFixtures}`,
)
assert(
  'matrix:row count',
  rows.length === expected.compile.compileRows,
  `${rows.length} != ${expected.compile.compileRows}`,
)

const summary = rows.map(r => ({
  rowId: r.rowId,
  sourceId: r.sourceId,
  mode: r.mode,
  vaporPath: r.vaporPath,
  pluginForceEligible: r.pluginForceEligible,
  pluginVapor: r.pluginVapor,
  effectiveVapor: r.effectiveVapor,
  descVapor: r.descriptor.vapor,
  stages: r.stages,
  composedComponent: r.composedComponent,
  scriptOk: r.script?.ok ?? null,
  scriptError: r.script?.error ?? null,
  vaporFlagInCode: r.script?.vaporFlagInCode ?? null,
  usesOpenBlock: r.script?.usesOpenBlock ?? null,
  templateOk: r.template?.ok ?? null,
  templateError: r.template?.error ?? r.template?.errors ?? null,
  vaporDirectOk: r.vaporDirect?.ok ?? null,
  vaporHelpers: r.vaporDirect?.helpers ?? null,
  scriptPath: r.scriptPath,
  templatePath: r.templatePath,
  vaporDirectPath: r.vaporDirectPath,
}))

const counts = {
  sourceFixtures: sources.length,
  modesPerDefaultSource: modes.length,
  extraSsrRows: 2,
  compileRows: rows.length,
  focusedAssertions: assertions.length,
  focusedAssertionFailures: assertionFailed,
  note: `${sources.length} source fixtures × ${modes.length} client modes + 2 SSR rows = ${rows.length} compile rows.`,
}

const portableRows = rows.map(r => {
  const copy = { ...r }
  return copy
})

writeFileSync(join(outDir, 'compile-results.json'), JSON.stringify(portableRows, null, 2))
writeFileSync(join(outDir, 'compile-summary.json'), JSON.stringify(summary, null, 2))
writeFileSync(join(outDir, 'compile-counts.json'), JSON.stringify(counts, null, 2))
writeFileSync(join(outDir, 'compile-assertions.json'), JSON.stringify({ assertionFailed, assertions }, null, 2))
writeFileSync(join(outDir, 'sources.json'), JSON.stringify(sources, null, 2))

const report = {
  counts,
  assertionFailed,
  failedAssertions: assertions.filter(a => !a.ok),
  allPassed: assertionFailed === 0,
}
console.log(JSON.stringify(report, null, 2))

if (assertionFailed !== 0) process.exitCode = 1
