import { spawnSync } from 'node:child_process'
import { createRequire } from 'node:module'
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join, relative, sep } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { parse, compileScript } from '@vue/compiler-sfc'
import { JSDOM } from 'jsdom'
import { portableRelativeSpecifier, rewriteSpecifiers } from './lib/rewrite-vue-imports.mjs'
import { defaultChannel, readBuildId, resolveVueDist } from './lib/runtime-builds.mjs'

const require = createRequire(import.meta.url)
const selfPath = fileURLToPath(import.meta.url)
const root = dirname(selfPath)
const expected = JSON.parse(readFileSync(join(root, 'expectations/counts.json'), 'utf8'))
const auditedVue = expected.auditedTuple.vue
const auditedCompilerSfc = expected.auditedTuple.compilerSfc

const args = process.argv.slice(2)
const negativeControl = args.includes('--negative-control')
const invalidSfcChild = args.includes('--invalid-sfc-child')
const buildIdx = args.indexOf('--runtime-build')
const requestedBuild = buildIdx >= 0 ? args[buildIdx + 1] : null

function installedCompilerSfcVersion() {
  return require('@vue/compiler-sfc/package.json').version
}

function repoRel(abs) {
  const rel = relative(join(root, '../..'), abs).split(sep).join('/')
  return rel
}

const fixtures = [
  {
    id: 'counter-click',
    source: `<script setup>
import { ref } from 'vue'
const n = ref(0)
</script>
<template>
  <button class="inc" @click="n++">{{ n }}</button>
</template>`,
    expectedEqual: true,
    steps: [
      { name: 'initial', expect: { '.inc': '0' } },
      { name: 'click', click: '.inc', expect: { '.inc': '1' } },
      { name: 'second-click', click: '.inc', expect: { '.inc': '2' } },
    ],
    teardown: true,
  },
  {
    id: 'v-if-toggle',
    source: `<script setup>
import { ref } from 'vue'
const ok = ref(true)
</script>
<template>
  <button class="toggle" @click="ok = !ok">t</button>
  <p v-if="ok" class="yes">yes</p>
  <p v-else class="no">no</p>
</template>`,
    expectedEqual: true,
    steps: [
      {
        name: 'initial',
        expectPresent: ['.yes'],
        expectAbsent: ['.no'],
      },
      {
        name: 'toggle-off',
        click: '.toggle',
        expectPresent: ['.no'],
        expectAbsent: ['.yes'],
      },
      {
        name: 'toggle-on',
        click: '.toggle',
        expectPresent: ['.yes'],
        expectAbsent: ['.no'],
      },
    ],
    teardown: true,
  },
  {
    id: 'keyed-list',
    source: `<script setup>
import { ref } from 'vue'
const items = ref([1, 2])
</script>
<template>
  <button class="push" @click="items.push(items.length + 1)">p</button>
  <button class="shift" @click="items.shift()">s</button>
  <ul class="list">
    <li v-for="item in items" :key="item" class="row">{{ item }}</li>
  </ul>
</template>`,
    expectedEqual: true,
    steps: [
      { name: 'initial', expectList: { '.row': ['1', '2'] } },
      { name: 'push', click: '.push', expectList: { '.row': ['1', '2', '3'] } },
      { name: 'shift', click: '.shift', expectList: { '.row': ['2', '3'] } },
    ],
    teardown: true,
  },
  {
    id: 'v-memo-stable-key',
    source: `<script setup>
import { ref } from 'vue'
const memoKey = ref(0)
const shown = ref(0)
</script>
<template>
  <div>
    <span class="memo" v-memo="[memoKey]">{{ shown }}</span>
    <span class="live">{{ shown }}</span>
    <button class="bump-shown" @click="shown++">s</button>
    <button class="bump-memo" @click="memoKey++">m</button>
  </div>
</template>`,
    expectedEqual: false,
    divergeAt: 'bump-shown-stable-memo',
    steps: [
      { name: 'initial', expect: { '.memo': '0', '.live': '0' } },
      {
        name: 'bump-shown-stable-memo',
        click: '.bump-shown',
        expectByMode: {
          vdom: { '.memo': '0', '.live': '1' },
          vapor: { '.memo': '1', '.live': '1' },
        },
      },
      { name: 'bump-memo-key', click: '.bump-memo', expect: { '.memo': '1', '.live': '1' } },
    ],
    teardown: true,
  },
  {
    id: 'v-once-vs-live',
    source: `<script setup>
import { ref } from 'vue'
const n = ref(0)
</script>
<template>
  <div>
    <span class="once" v-once>{{ n }}</span>
    <span class="live">{{ n }}</span>
    <button class="inc" @click="n++">i</button>
  </div>
</template>`,
    expectedEqual: true,
    steps: [
      { name: 'initial', expect: { '.once': '0', '.live': '0' } },
      { name: 'click', click: '.inc', expect: { '.once': '0', '.live': '1' } },
    ],
    teardown: true,
  },
]

const invalidFixture = {
  id: 'invalid-sfc-compile-failure',
  source: `<script setup>
export default { name: 'Invalid' }
</script>
<template><p>x</p></template>`,
  expectedEqual: true,
  steps: [{ name: 'initial', expect: { 'p': 'x' } }],
  teardown: true,
}

function expectedCheckpointsFor(list) {
  return list.reduce((n, fixture) => n + (fixture.steps.length + (fixture.teardown ? 1 : 0)) * 2, 0)
}

function compileInline(source, filename, vapor) {
  const { descriptor, errors } = parse(source, { filename })
  if (errors.length) {
    throw new Error(errors.map(e => e.message).join('\n'))
  }
  return compileScript(descriptor, {
    id: `data-v-${filename.replace(/\W/g, '')}`,
    vapor,
    inlineTemplate: true,
    isProd: true,
  })
}

function textOf(rootEl, selector) {
  const el = rootEl.querySelector(selector)
  return el ? el.textContent : null
}

function present(rootEl, selector) {
  return !!rootEl.querySelector(selector)
}

function listText(rootEl, selector) {
  return [...rootEl.querySelectorAll(selector)].map(el => el.textContent)
}

function checkExpect(rootEl, expect) {
  const failures = []
  for (const [selector, value] of Object.entries(expect)) {
    const got = textOf(rootEl, selector)
    if (got !== value) {
      failures.push(`${selector} expected ${JSON.stringify(value)} got ${JSON.stringify(got)}`)
    }
  }
  return failures
}

function runChecks(rootEl, step, mode) {
  const failures = []
  const expect = step.expectByMode?.[mode] ?? step.expect
  if (expect) failures.push(...checkExpect(rootEl, expect))
  for (const selector of step.expectPresent || []) {
    if (!present(rootEl, selector)) failures.push(`missing ${selector}`)
  }
  for (const selector of step.expectAbsent || []) {
    if (present(rootEl, selector)) failures.push(`unexpected ${selector}`)
  }
  if (step.expectList) {
    for (const [selector, value] of Object.entries(step.expectList)) {
      const got = listText(rootEl, selector)
      if (JSON.stringify(got) !== JSON.stringify(value)) {
        failures.push(`${selector} list expected ${JSON.stringify(value)} got ${JSON.stringify(got)}`)
      }
    }
  }
  return failures
}

function installDom() {
  const dom = new JSDOM('<!doctype html><html><body></body></html>', {
    url: 'http://localhost/',
    pretendToBeVisual: true,
  })
  const { window } = dom
  globalThis.window = window
  globalThis.document = window.document
  globalThis.Node = window.Node
  globalThis.Element = window.Element
  globalThis.Comment = window.Comment
  globalThis.Text = window.Text
  globalThis.DocumentFragment = window.DocumentFragment
  globalThis.HTMLElement = window.HTMLElement
  globalThis.SVGElement = window.SVGElement
  globalThis.MutationObserver = window.MutationObserver
  globalThis.requestAnimationFrame = cb => setTimeout(() => cb(Date.now()), 0)
  globalThis.cancelAnimationFrame = id => clearTimeout(id)
  return window
}

async function runHarness({ channel, fixtureList, outDir, vueAbs, buildId }) {
  const modDir = join(outDir, 'modules')
  rmSync(outDir, { recursive: true, force: true })
  mkdirSync(modDir, { recursive: true })

  installDom()
  const vueSpec = portableRelativeSpecifier(join(modDir, 'dummy.mjs'), vueAbs)
  const Vue = await import(pathToFileURL(vueAbs).href)
  const { createApp, createVaporApp, nextTick } = Vue
  if (typeof createVaporApp !== 'function' || typeof createApp !== 'function') {
    throw new Error(`runtime build ${buildId.file} is missing createApp/createVaporApp`)
  }
  const loadedVueVersion = Vue.version
  const compilerSfcVersion = installedCompilerSfcVersion()
  const identityMismatches = []
  if (loadedVueVersion !== auditedVue) {
    identityMismatches.push(`Vue.version ${loadedVueVersion} != audited ${auditedVue}`)
  }
  if (compilerSfcVersion !== auditedCompilerSfc) {
    identityMismatches.push(
      `@vue/compiler-sfc package.version ${compilerSfcVersion} != audited ${auditedCompilerSfc}`,
    )
  }
  if (buildId.version !== auditedVue) {
    identityMismatches.push(`build header version ${buildId.version} != audited ${auditedVue}`)
  }
  if (identityMismatches.length) {
    throw new Error(`runtime identity mismatch: ${identityMismatches.join('; ')}`)
  }

  const results = []
  let failed = 0
  let checkpointCount = 0
  let checkpointsPassed = 0
  const coverageErrors = []

  function recordFailure(caseResult, message) {
    failed += 1
    if (caseResult) {
      caseResult.passed = false
      caseResult.errors.push(message)
    } else {
      coverageErrors.push(message)
    }
  }

  for (const fixture of fixtureList) {
    const caseResult = {
      id: fixture.id,
      expectedEqual: fixture.expectedEqual,
      divergeAt: fixture.divergeAt ?? null,
      modes: {},
      equality: [],
      identity: {},
      passed: true,
      errors: [],
    }

    const compiled = {}
    for (const mode of ['vdom', 'vapor']) {
      try {
        const out = compileInline(fixture.source, `${fixture.id}.vue`, mode === 'vapor')
        const file = join(modDir, `${fixture.id}.${mode}.mjs`)
        const rewritten = await rewriteSpecifiers(out.content, { vue: vueSpec })
        if (rewritten.includes('file:') || /file:\//.test(rewritten)) {
          throw new Error('rewritten module contains a file URL')
        }
        writeFileSync(file, rewritten)
        writeFileSync(join(outDir, `${fixture.id}.${mode}.compiled.js`), out.content)
        compiled[mode] = {
          content: out.content,
          modulePath: repoRel(file),
          vaporFlag: /__vapor:\s*true/.test(out.content),
        }
      } catch (e) {
        recordFailure(caseResult, `${mode} compile: ${e.message}`)
      }
    }

    const snapshots = { vdom: [], vapor: [] }

    for (const mode of ['vdom', 'vapor']) {
      if (!compiled[mode]) continue
      const container = document.createElement('div')
      container.id = `${fixture.id}-${mode}`
      document.body.appendChild(container)
      let mod
      try {
        const href = pathToFileURL(join(modDir, `${fixture.id}.${mode}.mjs`)).href
        mod = await import(href)
      } catch (e) {
        recordFailure(caseResult, `${mode} import: ${e.message}`)
        container.remove()
        continue
      }
      const Comp = mod.default
      const compVapor = Comp?.__vapor === true
      const app = mode === 'vapor' ? createVaporApp(Comp) : createApp(Comp)
      try {
        app.mount(container)
        await nextTick()
      } catch (e) {
        recordFailure(caseResult, `${mode} mount: ${e.message}`)
        container.remove()
        continue
      }

      const vaporFlag = compiled[mode].vaporFlag
      const appVapor = app.vapor === true
      caseResult.identity[mode] = { vaporFlag, appVapor, compVapor }
      if (mode === 'vapor') {
        if (!vaporFlag) recordFailure(caseResult, 'vapor compile missing __vapor: true')
        if (!compVapor) recordFailure(caseResult, 'vapor component missing Comp.__vapor === true')
        if (!appVapor) recordFailure(caseResult, 'createVaporApp did not set app.vapor')
      } else {
        if (vaporFlag) recordFailure(caseResult, 'vdom compile unexpectedly contains __vapor: true')
        if (compVapor) recordFailure(caseResult, 'vdom component unexpectedly has Comp.__vapor === true')
        if (appVapor) recordFailure(caseResult, 'createApp unexpectedly set app.vapor')
      }

      const modeResult = {
        vaporFlag,
        appVapor,
        compVapor,
        steps: [],
        unmountedHtml: null,
      }

      for (const step of fixture.steps) {
        if (step.click) {
          const target = container.querySelector(step.click)
          if (!target) {
            checkpointCount += 1
            recordFailure(caseResult, `${mode} ${step.name}: no ${step.click}`)
            modeResult.steps.push({ name: step.name, ok: false, failures: [`no ${step.click}`] })
            continue
          }
          target.click()
          await nextTick()
        }
        const failures = runChecks(container, step, mode)
        snapshots[mode].push({ name: step.name, html: container.innerHTML, text: container.textContent })
        checkpointCount += 1
        const ok = failures.length === 0
        if (ok) checkpointsPassed += 1
        else recordFailure(caseResult, `${mode} ${step.name}: ${failures.join('; ')}`)
        modeResult.steps.push({
          name: step.name,
          ok,
          failures,
          html: container.innerHTML,
          text: container.textContent,
        })
      }

      if (fixture.teardown) {
        app.unmount()
        await nextTick()
        modeResult.unmountedHtml = container.innerHTML
        const empty = container.innerHTML === '' || container.childNodes.length === 0
        checkpointCount += 1
        if (empty) checkpointsPassed += 1
        else recordFailure(caseResult, `${mode} teardown: container not empty`)
        modeResult.steps.push({
          name: 'teardown',
          ok: empty,
          failures: empty ? [] : [`container not empty: ${JSON.stringify(container.innerHTML)}`],
          html: container.innerHTML,
        })
      }

      container.remove()
      caseResult.modes[mode] = modeResult
    }

    for (const mode of ['vdom', 'vapor']) {
      if (!caseResult.modes[mode]) {
        recordFailure(caseResult, `missing mode ${mode}`)
      }
    }

    const stepNames = fixture.steps.map(s => s.name)
    for (const name of stepNames) {
      const v = snapshots.vdom.find(s => s.name === name)
      const p = snapshots.vapor.find(s => s.name === name)
      if (!v || !p) {
        recordFailure(caseResult, `incomplete pair snapshot at ${name}`)
        continue
      }
      const equalText = v.text === p.text
      caseResult.equality.push({
        step: name,
        equalText,
        vdomText: v.text,
        vaporText: p.text,
      })
    }

    if (fixture.expectedEqual) {
      for (const eq of caseResult.equality) {
        if (!eq.equalText) {
          recordFailure(caseResult, `expected equal text at ${eq.step}`)
        }
      }
    } else if (fixture.divergeAt) {
      const diverge = caseResult.equality.find(eq => eq.step === fixture.divergeAt)
      if (!diverge) recordFailure(caseResult, `missing divergence step ${fixture.divergeAt}`)
      else if (diverge.equalText) {
        recordFailure(caseResult, `expected v-memo divergence at ${fixture.divergeAt}`)
      }
    }

    results.push(caseResult)
  }

  const expectedCheckpoints = expectedCheckpointsFor(fixtureList)
  if (checkpointCount < expectedCheckpoints) {
    recordFailure(null, `checkpoint coverage ${checkpointCount} < expected ${expectedCheckpoints}`)
  }
  if (!invalidSfcChild && fixtureList === fixtures) {
    if (fixtureList.length !== expected.runtime.fixturePairs) {
      recordFailure(
        null,
        `fixture pair count ${fixtureList.length} != documented ${expected.runtime.fixturePairs}`,
      )
    }
    const missingPair = results.find(r => !r.modes.vdom || !r.modes.vapor)
    if (missingPair) {
      recordFailure(null, `fixture ${missingPair.id} is missing a vdom/vapor pair`)
    }
    if (checkpointCount !== expected.runtime.checkpoints) {
      recordFailure(
        null,
        `checkpoint count ${checkpointCount} != documented ${expected.runtime.checkpoints}`,
      )
    }
  }

  const allPassed = failed === 0
  const report = {
    vue: {
      expected: auditedVue,
      loaded: loadedVueVersion,
    },
    runtimeBuild: buildId,
    compiler: {
      expected: auditedCompilerSfc,
      packageVersion: compilerSfcVersion,
    },
    modes: [
      'vdom=createApp + compileScript(vapor:false, inlineTemplate, isProd)',
      'vapor=createVaporApp + compileScript(vapor:true, inlineTemplate, isProd)',
    ],
    fixtures: fixtureList.length,
    checkpointCount,
    checkpointsPassed,
    expectedCheckpoints,
    failed,
    coverageErrors,
    allPassed,
    results,
  }
  writeFileSync(join(outDir, 'results.json'), JSON.stringify(report, null, 2))
  return report
}

if (negativeControl) {
  const child = spawnSync(process.execPath, [selfPath, '--invalid-sfc-child'], {
    cwd: root,
    encoding: 'utf8',
  })
  let parsed = null
  let parseError = null
  try {
    parsed = JSON.parse((child.stdout || '').trim())
  } catch (e) {
    parseError = e.message
  }
  const errors = parsed?.cases?.flatMap(r => r.errors) ?? []
  const compileFailed = errors.some(e => typeof e === 'string' && e.includes('compile:'))
  const allPassedFalse = parsed?.allPassed === false
  const statusOk = child.status === 1
  const ok = statusOk && allPassedFalse && compileFailed && parsed?.failed > 0
  console.log(
    JSON.stringify(
      {
        negativeControl: true,
        childStatus: child.status,
        compileFailed,
        childAllPassed: parsed?.allPassed ?? null,
        failed: parsed?.failed ?? null,
        parseError,
        stderr: child.stderr || '',
        ok,
        errors,
      },
      null,
      2,
    ),
  )
  process.exitCode = ok ? 0 : 1
} else {
  if (requestedBuild && requestedBuild !== 'prod' && requestedBuild !== 'dev') {
    console.error(`unknown --runtime-build ${requestedBuild}`)
    process.exit(2)
  }

  const channel = requestedBuild || defaultChannel(root)
  const vueAbs = resolveVueDist(root, channel)
  const buildId = readBuildId(vueAbs, channel)
  const outDir = invalidSfcChild
    ? join(root, 'output/runtime/invalid-sfc-child')
    : join(root, 'output/runtime', channel)

  const report = await runHarness({
    channel,
    fixtureList: invalidSfcChild ? [invalidFixture] : fixtures,
    outDir,
    vueAbs,
    buildId,
  })

  const summary = {
    invalidSfcChild,
    checkpointCount: report.checkpointCount,
    checkpointsPassed: report.checkpointsPassed,
    expectedCheckpoints: report.expectedCheckpoints,
    failed: report.failed,
    allPassed: report.allPassed,
    coverageErrors: report.coverageErrors,
    vue: report.vue,
    compiler: report.compiler,
    runtimeBuild: report.runtimeBuild,
    cases: report.results.map(r => ({
      id: r.id,
      passed: r.passed,
      errors: r.errors,
      identity: r.identity,
      equality: r.equality,
      vapor: r.modes.vapor?.steps.map(s => ({ name: s.name, ok: s.ok, failures: s.failures })),
      vdom: r.modes.vdom?.steps.map(s => ({ name: s.name, ok: s.ok, failures: s.failures })),
    })),
  }

  console.log(JSON.stringify(summary, null, 2))
  if (!report.allPassed) process.exitCode = 1
}
