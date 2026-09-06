import { createHash } from 'node:crypto'
import { existsSync, readFileSync } from 'node:fs'
import { basename, join } from 'node:path'

export const BUILD_FILES = {
  prod: 'vue.runtime-with-vapor.esm-browser.prod.js',
  dev: 'vue.runtime-with-vapor.esm-browser.js',
}

export function resolveVueDist(root, channel) {
  const file = BUILD_FILES[channel]
  if (!file) throw new Error(`unknown runtime channel ${channel}`)
  const abs = join(root, 'node_modules/vue/dist', file)
  if (!existsSync(abs)) {
    throw new Error(`missing inlined vapor browser build: vue/dist/${file}`)
  }
  return abs
}

export function defaultChannel(root) {
  const prod = join(root, 'node_modules/vue/dist', BUILD_FILES.prod)
  return existsSync(prod) ? 'prod' : 'dev'
}

export function readBuildId(absPath, channel) {
  const buf = readFileSync(absPath)
  const head = buf.subarray(0, 800).toString('utf8')
  const version = (head.match(/vue v([^\s*]+)/i) || [])[1] || null
  return {
    channel,
    file: basename(absPath),
    packagePath: `vue/dist/${basename(absPath)}`,
    version,
    bytes: buf.length,
    sha256: createHash('sha256').update(buf).digest('hex'),
  }
}
