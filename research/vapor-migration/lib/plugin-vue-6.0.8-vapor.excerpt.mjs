/**
 * Excerpt of @vitejs/plugin-vue@6.0.8 force-mode helpers.
 *
 * Source: https://github.com/vitejs/vite-plugin-vue/blob/d8ff7d0e8f557a7c1975c07b30e232c69bdbbc03/packages/plugin-vue/src/utils/vapor.ts
 * Published dist: plugin-vue@6.0.8 `dist/index.mjs` (`isVaporMode` / `canForceVaporMode`).
 * Template-only seed: `genScriptCode` initial assignment in the same dist file
 * (`const _sfc_main = { __vapor: true }` when vapor mode is on and no script
 * has been inlined yet).
 *
 * The research harness invokes `@vue/compiler-sfc` / `@vue/compiler-vapor`
 * and consults these excerpted functions for
 * plugin-vue 6.0.8 eligibility facts. `compileScript({ vapor: true })` is a
 * compiler-API path and is labeled separately when it diverges from plugin force.
 */

export function isVaporMode(descriptor, options) {
  if (descriptor.vapor) return true
  if (options.features?.vapor) return canForceVaporMode(descriptor)
  return false
}

export function canForceVaporMode(descriptor) {
  if (descriptor.filename.endsWith('.vue')) {
    if (descriptor.scriptSetup) return true
    if (descriptor.script) return false
  }
  return true
}

/**
 * Plugin-pipeline emulation of the template-only component object seed.
 * compiler-sfc does not emit this object for a template-only SFC.
 */
export function pluginTemplateOnlyComponentSeed(descriptor, options) {
  if (descriptor.script || descriptor.scriptSetup) {
    return {
      kind: 'not-applicable-script-present',
      pipeline: 'plugin-vue-6.0.8-emulation',
    }
  }
  const vapor = isVaporMode(descriptor, options)
  return {
    kind: 'plugin-pipeline-emulation',
    pipeline: 'plugin-vue-6.0.8-emulation',
    source: 'genScriptCode initial object before script replacement',
    object: vapor ? { __vapor: true } : {},
  }
}
