import { parse, compileScript, compileTemplate } from '@vue/compiler-sfc'
import { compile as compileVapor } from '@vue/compiler-vapor'
import {
  canForceVaporMode,
  isVaporMode,
  pluginTemplateOnlyComponentSeed,
} from './plugin-vue-6.0.8-vapor.excerpt.mjs'

function summarizeErrors(errors) {
  return (errors || []).map(e => ({
    message: e.message || String(e),
    code: e.code ?? null,
    loc: e.loc
      ? {
          start: e.loc.start,
          end: e.loc.end,
        }
      : null,
  }))
}

function descriptorShape(descriptor) {
  return {
    vapor: !!descriptor.vapor,
    filename: descriptor.filename,
    hasScript: !!descriptor.script,
    hasScriptSetup: !!descriptor.scriptSetup,
    hasTemplate: !!descriptor.template,
    scriptAttrs: descriptor.script?.attrs ?? null,
    scriptSetupAttrs: descriptor.scriptSetup?.attrs ?? null,
    templateAttrs: descriptor.template?.attrs ?? null,
    scriptLang: descriptor.script?.lang ?? null,
    scriptSetupLang: descriptor.scriptSetup?.lang ?? null,
  }
}

function scriptFacts(content) {
  return {
    vaporFlagInCode:
      content.includes('__vapor: true') || content.includes('defineVaporComponent'),
    usesDefineVaporComponent: content.includes('defineVaporComponent'),
    usesDefineComponent: /defineComponent\(/.test(content),
    usesOpenBlock: content.includes('openBlock'),
    usesVaporTemplateHelper:
      content.includes('template as _template') ||
      content.includes('function template') ||
      (/from ['"]vue['"]/.test(content) && content.includes('_template(')),
    usesObjectAssign: content.includes('Object.assign'),
    hasMemoHelper: /(?:withMemo|vMemo|_memo)\(/.test(content),
    renderEffectIncludesMemoKey: /_renderEffect\([\s\S]*?memoKey/.test(content),
    renderEffectIncludesShown: /_renderEffect\([\s\S]*?shown/.test(content),
  }
}

export function compileSfc(source, options) {
  const {
    filename,
    featuresVapor = false,
    ssr = false,
    inlineTemplate = true,
    isProd = true,
    naiveCompileScriptVapor = false,
  } = options

  const { descriptor, errors: parseErrors } = parse(source, { filename })
  const pluginOptions = { features: { vapor: featuresVapor } }
  const pluginForceEligible = canForceVaporMode(descriptor)
  const pluginVapor = isVaporMode(descriptor, pluginOptions)
  const vapor = naiveCompileScriptVapor ? true : pluginVapor
  const hasScript = !!(descriptor.scriptSetup || descriptor.script)

  const result = {
    filename,
    featuresVapor,
    ssr,
    inlineTemplate,
    isProd,
    naiveCompileScriptVapor,
    vaporPath: naiveCompileScriptVapor
      ? 'compiler-sfc-api-vapor-true'
      : 'plugin-vue-6.0.8-eligibility',
    pluginForceEligible,
    pluginVapor,
    effectiveVapor: vapor,
    descriptor: descriptorShape(descriptor),
    parseErrors: summarizeErrors(parseErrors),
    stages: {
      parse: {
        ok: parseErrors.length === 0,
        errorCount: parseErrors.length,
      },
      script: { present: hasScript, invoked: false, ok: null, error: null },
      template: { present: !!descriptor.template, invoked: false, ok: null, error: null },
      ssr: { used: !!ssr, compiler: ssr ? 'compiler-ssr-via-compileTemplate' : null },
      vaporDirect: { invoked: false, ok: null, error: null },
    },
    composedComponent: pluginTemplateOnlyComponentSeed(descriptor, pluginOptions),
    script: null,
    template: null,
    vaporDirect: null,
  }

  if (hasScript) {
    result.stages.script.invoked = true
    try {
      const compiled = compileScript(descriptor, {
        id: `data-v-${filename.replace(/[^a-zA-Z0-9_-]/g, '')}`,
        vapor,
        inlineTemplate,
        isProd,
        templateOptions: { ssr },
      })
      result.script = {
        ok: true,
        content: compiled.content,
        bindings: compiled.bindings ?? null,
        ...scriptFacts(compiled.content),
      }
      result.stages.script.ok = true
    } catch (e) {
      result.script = { ok: false, error: e.message, content: null, bindings: null }
      result.stages.script.ok = false
      result.stages.script.error = e.message
    }
  } else {
    result.stages.script.ok = null
  }

  const wantSeparateTemplate =
    descriptor.template && (!inlineTemplate || !hasScript || result.script?.ok === false)
  if (descriptor.template && (wantSeparateTemplate || !inlineTemplate)) {
    result.stages.template.invoked = true
    try {
      const compiled = compileTemplate({
        source: descriptor.template.content,
        ast: descriptor.template.ast,
        filename,
        id: `data-v-${filename.replace(/[^a-zA-Z0-9_-]/g, '')}`,
        vapor,
        ssr,
        isProd,
        compilerOptions: {
          bindingMetadata: result.script?.bindings ?? {},
        },
      })
      result.template = {
        ok: compiled.errors.length === 0,
        errors: summarizeErrors(compiled.errors),
        code: compiled.code,
        preamble: compiled.preamble ?? null,
        usesVaporTemplateHelper:
          typeof compiled.code === 'string' &&
          (compiled.code.includes('template as _template') ||
            compiled.code.includes('_template(')),
        usesOpenBlock: typeof compiled.code === 'string' && compiled.code.includes('openBlock'),
      }
      result.stages.template.ok = result.template.ok
      if (!result.template.ok) {
        result.stages.template.error = result.template.errors
      }
    } catch (e) {
      result.template = { ok: false, error: e.message, code: null }
      result.stages.template.ok = false
      result.stages.template.error = e.message
    }
  }

  if (descriptor.template && vapor && !ssr) {
    result.stages.vaporDirect.invoked = true
    try {
      const gen = compileVapor(descriptor.template.content, {
        mode: 'module',
        prefixIdentifiers: true,
        isProd,
        bindingMetadata: result.script?.bindings ?? {},
      })
      result.vaporDirect = {
        ok: true,
        helpers: [...(gen.helpers || [])].map(String),
        code: gen.code,
      }
      result.stages.vaporDirect.ok = true
    } catch (e) {
      result.vaporDirect = { ok: false, error: e.message, helpers: [], code: null }
      result.stages.vaporDirect.ok = false
      result.stages.vaporDirect.error = e.message
    }
  }

  return result
}
