import { dirname, relative, sep } from 'node:path'
import { init, parse } from 'es-module-lexer'

let ready
function ensureInit() {
  ready ??= init
  return ready
}

/** POSIX relative specifier from `fromFile` to `toFile`, always `./` or `../`. */
export function portableRelativeSpecifier(fromFile, toFile) {
  let rel = relative(dirname(fromFile), toFile).split(sep).join('/')
  if (!rel.startsWith('.')) rel = `./${rel}`
  return rel
}

/**
 * Rewrite ESM specifiers with es-module-lexer (not a source regex).
 * `map` is specifier string → replacement specifier string.
 * The lexer spans are the unquoted specifier, so original quoting is preserved.
 */
export async function rewriteSpecifiers(code, map) {
  await ensureInit()
  const [imports] = parse(code)
  let out = code
  for (let i = imports.length - 1; i >= 0; i--) {
    const im = imports[i]
    if (!im.n) continue
    const next = map[im.n]
    if (!next) continue
    if (next.includes('file:') || next.includes('\\') || /file:\//.test(next)) {
      throw new Error(`refusing non-portable specifier replacement: ${next}`)
    }
    out = out.slice(0, im.s) + next + out.slice(im.e)
  }
  return out
}

export async function listSpecifiers(code) {
  await ensureInit()
  const [imports] = parse(code)
  return imports.filter(im => im.n).map(im => im.n)
}
