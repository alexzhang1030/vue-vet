import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs'
import { dirname, join, relative, sep } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = dirname(fileURLToPath(import.meta.url))
const repo = join(root, '../..')

const publicFiles = [
  'docs/research/vapor-migration.md',
  'research/vapor-migration/README.md',
  '.agents/docs/README.md',
  '.agents/docs/technology-stack.md',
]

const forbidden = [
  /Codex/i,
  /Grok/i,
  /\bscratch\b/i,
  /file:\/\//i,
  /\/Users\//,
  /\/private\/tmp/,
  /\/tmp\/vue-vet/,
]

const requiredFiles = [
  'docs/research/vapor-migration.md',
  'research/vapor-migration/README.md',
  'research/vapor-migration/package.json',
  'research/vapor-migration/package-lock.json',
  'research/vapor-migration/compile-cases.mjs',
  'research/vapor-migration/runtime-diff.mjs',
  'research/vapor-migration/lib/compile-sfc.mjs',
  'research/vapor-migration/lib/plugin-vue-6.0.8-vapor.excerpt.mjs',
  'research/vapor-migration/lib/rewrite-vue-imports.mjs',
  'research/vapor-migration/expectations/counts.json',
  'research/vapor-migration/expectations/compiler-sfc-3.5.42.json',
]

const failures = []

for (const rel of requiredFiles) {
  if (!existsSync(join(repo, rel))) failures.push(`missing ${rel}`)
}

function walk(dir, acc = []) {
  for (const name of readdirSync(dir)) {
    if (name === 'node_modules' || name === 'output') continue
    const abs = join(dir, name)
    const st = statSync(abs)
    if (st.isDirectory()) walk(abs, acc)
    else acc.push(abs)
  }
  return acc
}

for (const rel of publicFiles) {
  const abs = join(repo, rel)
  const text = readFileSync(abs, 'utf8')
  for (const re of forbidden) {
    if (re.test(text)) failures.push(`${rel} matches ${re}`)
  }
  const links = [...text.matchAll(/\[[^\]]+\]\(([^)]+)\)/g)].map(m => m[1])
  for (const href of links) {
    if (/^https?:\/\//.test(href) || href.startsWith('#')) continue
    const cleaned = href.split('#')[0]
    if (!cleaned) continue
    const from = dirname(abs)
    const target = join(from, cleaned)
    if (!existsSync(target)) failures.push(`${rel} broken link ${href}`)
  }
}

for (const abs of walk(root)) {
  const rel = relative(repo, abs).split(sep).join('/')
  if (rel.endsWith('check-published-surface.mjs')) continue
  const text = readFileSync(abs, 'utf8')
  if (text.includes('file://')) failures.push(`${rel} contains a file URL`)
  if (abs.endsWith('.mjs') || abs.endsWith('.json') || abs.endsWith('.md')) {
    if (/\/Users\/|\/private\/tmp\//.test(text)) {
      failures.push(`${rel} contains a host path`)
    }
  }
}

const gitignore = readFileSync(join(root, '.gitignore'), 'utf8')
if (!gitignore.includes('node_modules/')) failures.push('research gitignore missing node_modules/')
if (!gitignore.includes('output/')) failures.push('research gitignore missing output/')

const pkg = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'))
for (const name of ['vue', '@vue/compiler-sfc', '@vue/compiler-vapor', '@vue/runtime-vapor']) {
  if (pkg.dependencies[name] !== '3.6.0-rc.7') {
    failures.push(`package.json ${name} is ${pkg.dependencies[name]}`)
  }
}

console.log(
  JSON.stringify(
    {
      checkedFiles: publicFiles.length + requiredFiles.length,
      failures,
      ok: failures.length === 0,
    },
    null,
    2,
  ),
)
if (failures.length) process.exitCode = 1
