//! Vize-powered Vue SFC analysis → Vue Vet-owned facts.
//!
//! Parses with `vize_croquis::sfc` (never `vize_atelier_sfc`). Script blocks
//! delegate to [`vue_vet_oxc`]. Prefer [`analyze_sfc_with_facts`] /
//! [`analyze_sfc_facts_reusing`]; Vize AST must not escape this crate.

use std::path::Path;
use std::sync::Arc;

use thiserror::Error;
use vize_croquis::sfc::{SfcDescriptor, SfcParseOptions, parse_sfc};
use vue_vet_core::{
  ScriptBlockFacts, ScriptFacts, ScriptKind, SfcFacts, TemplateFacts, content_digest,
};
use vue_vet_oxc::{AnalyzeScriptError, analyze_module_source};
use vue_vet_reactivity::ModuleSource;

#[derive(Debug, Error)]
pub enum AnalyzeError {
  #[error("Vize could not parse the SFC: {0}")]
  Parse(String),
  #[error("Vize could not parse the template: {0}")]
  Template(String),
  #[error(transparent)]
  Script(#[from] AnalyzeScriptError),
}

#[derive(Clone, Debug)]
pub struct AnalyzedSfc {
  pub facts: SfcFacts,
  /// Preferred script block for cross-module reactivity (`script setup` > `script`).
  pub module_source: Option<ModuleSource>,
  /// Ordinary `<script>` block when dual-script SFCs also have `<script setup>`.
  /// Id is `{path}#script` so both blocks re-trace with module seeds independently.
  pub ordinary_module_source: Option<ModuleSource>,
  /// Content + absolute span fingerprints for block-level reuse.
  pub revisions: SfcBlockRevisions,
}

/// Fingerprints for SFC blocks that can be reused across edits.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SfcBlockRevisions {
  pub template: Option<BlockFingerprint>,
  pub script: Option<BlockFingerprint>,
  pub script_setup: Option<BlockFingerprint>,
}

/// Content digest plus absolute location — location drift forces re-extract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockFingerprint {
  pub content_digest: String,
  pub start: usize,
  pub end: usize,
}

/// Analyze one Vue SFC and retain its dependency-neutral project facts.
///
/// # Errors
///
/// Returns the same deterministic parse and semantic errors as
/// [`analyze_sfc_facts_reusing`].
pub fn analyze_sfc_with_facts(path: &Path, source: &str) -> Result<AnalyzedSfc, AnalyzeError> {
  analyze_sfc_facts_reusing(path, source, None)
}

/// Analyze an SFC, reusing unchanged template/script blocks from `previous`.
///
/// Style-only edits that do not move other blocks reuse the prior analysis
/// entirely. Template-only or script-only edits rebuild just the dirty blocks.
///
/// # Errors
///
/// Returns the same parse / template / script errors as [`analyze_sfc_with_facts`].
pub fn analyze_sfc_facts_reusing(
  path: &Path,
  source: &str,
  previous: Option<&AnalyzedSfc>,
) -> Result<AnalyzedSfc, AnalyzeError> {
  // Index only — avoid `SourceContext::new(&str)` copying the SFC into `Arc<str>`.
  let line_index = Arc::new(vue_vet_core::LineIndex::new(source));
  span::install_line_index(line_index);
  let result = analyze_sfc_facts_inner(path, source, previous);
  span::clear_line_index();
  result
}

fn analyze_sfc_facts_inner(
  path: &Path,
  source: &str,
  previous: Option<&AnalyzedSfc>,
) -> Result<AnalyzedSfc, AnalyzeError> {
  let descriptor = parse_sfc(source, SfcParseOptions::default())
    .map_err(|error| AnalyzeError::Parse(error.message.into()))?;
  let revisions = revisions_from_descriptor(&descriptor);
  if let Some(previous) = previous
    && previous.revisions == revisions
  {
    let (module_source, ordinary_module_source) = dual_module_sources(path, source, &descriptor);
    let module_source = attach_reused_summaries(module_source, previous.module_source.as_ref());
    let ordinary_module_source =
      attach_reused_summaries(ordinary_module_source, previous.ordinary_module_source.as_ref());
    let mut facts = previous.facts.clone();
    // Style is not in `revisions`. Refresh `v-bind(ident)` expressions so a
    // style-only ident change still joins, while color-only CSS stays equal.
    if style::refresh_style_v_bind_expressions(source, &descriptor, &mut facts.template) {
      let module_id = path.to_string_lossy().replace('\\', "/");
      for block in &mut facts.script.blocks {
        let graph = Arc::make_mut(&mut block.reactivity_graph);
        graph.join_template_reads(&facts.template);
        graph.set_module_id(module_id.clone());
      }
    }
    return Ok(AnalyzedSfc { facts, module_source, ordinary_module_source, revisions });
  }

  let (mut module_source, mut ordinary_module_source) =
    dual_module_sources(path, source, &descriptor);
  let has_script_setup = descriptor.script_setup.is_some();
  let reuse_template = previous.is_some_and(|prev| prev.revisions.template == revisions.template);
  let reuse_script = previous.is_some_and(|prev| prev.revisions.script == revisions.script);
  let reuse_setup =
    previous.is_some_and(|prev| prev.revisions.script_setup == revisions.script_setup);

  // Reuse a prior template only when script blocks are also reused. JSX from
  // script is merged into `template`; a script rebuild must start from a clean
  // Vize-only surface so stale JSX facts are not retained.
  let mut template = if reuse_template && reuse_script && reuse_setup {
    previous.map(|prev| prev.facts.template.clone()).unwrap_or_default()
  } else if let Some(template) = descriptor.template.as_ref() {
    template::extract_template_facts(source, &template.content, template.loc.start)?
  } else {
    TemplateFacts::default()
  };
  style::refresh_style_v_bind_expressions(source, &descriptor, &mut template);

  let mut script = ScriptFacts::default();
  let mut script_rebuilt = false;
  if let Some(block) = descriptor.script {
    let lang = block.lang.as_deref().unwrap_or("js");
    let can_reuse_script = (reuse_template || !matches!(lang, "jsx" | "tsx")) && reuse_script;
    let (script_facts, summary) = if can_reuse_script
      && let Some(previous) = previous
      && let Some(facts) = previous_script_block(&previous.facts, ScriptKind::Script)
    {
      let summary = if has_script_setup {
        previous.ordinary_module_source.as_ref().and_then(ModuleSource::module_summary)
      } else {
        previous.module_source.as_ref().and_then(ModuleSource::module_summary)
      };
      (facts.clone(), summary)
    } else {
      script_rebuilt = true;
      // `block.loc.start/end` are absolute offsets into the original SFC source.
      let analysis =
        analyze_module_source(source, &block.content, block.loc.start, lang, ScriptKind::Script)?;
      merge_jsx_template_facts(&mut template, analysis.template_facts);
      (analysis.script_facts, Some(analysis.module_trace))
    };
    let target = if has_script_setup { &mut ordinary_module_source } else { &mut module_source };
    if let Some(module) = target.take() {
      *target = Some(match summary {
        Some(summary) => module.with_module_summary(summary),
        None => module,
      });
    }
    script.blocks.push(script_facts);
  }
  if let Some(block) = descriptor.script_setup {
    let lang = block.lang.as_deref().unwrap_or("js");
    let can_reuse_setup = (reuse_template || !matches!(lang, "jsx" | "tsx")) && reuse_setup;
    let (script_facts, summary) = if can_reuse_setup
      && let Some(previous) = previous
      && let Some(facts) = previous_script_block(&previous.facts, ScriptKind::Setup)
    {
      (facts.clone(), previous.module_source.as_ref().and_then(ModuleSource::module_summary))
    } else {
      script_rebuilt = true;
      let analysis =
        analyze_module_source(source, &block.content, block.loc.start, lang, ScriptKind::Setup)?;
      merge_jsx_template_facts(&mut template, analysis.template_facts);
      (analysis.script_facts, Some(analysis.module_trace))
    };
    if let Some(module) = module_source.take() {
      module_source = Some(match summary {
        Some(summary) => module.with_module_summary(summary),
        None => module,
      });
    }
    script.blocks.push(script_facts);
  }

  // Join when template or any script block was rebuilt; full reuse already joined.
  let needs_join = !reuse_template || script_rebuilt;
  if needs_join {
    let module_id = path.to_string_lossy().replace('\\', "/");
    for block in &mut script.blocks {
      let graph = Arc::make_mut(&mut block.reactivity_graph);
      graph.join_template_reads(&template);
      graph.set_module_id(module_id.clone());
    }
  }
  Ok(AnalyzedSfc {
    facts: SfcFacts { template, script },
    module_source,
    ordinary_module_source,
    revisions,
  })
}

fn revisions_from_descriptor(descriptor: &SfcDescriptor<'_>) -> SfcBlockRevisions {
  SfcBlockRevisions {
    template: descriptor
      .template
      .as_ref()
      .map(|block| fingerprint(block.content.as_ref(), block.loc.start, block.loc.end)),
    script: descriptor
      .script
      .as_ref()
      .map(|block| fingerprint(block.content.as_ref(), block.loc.start, block.loc.end)),
    script_setup: descriptor
      .script_setup
      .as_ref()
      .map(|block| fingerprint(block.content.as_ref(), block.loc.start, block.loc.end)),
  }
}

fn fingerprint(content: &str, start: usize, end: usize) -> BlockFingerprint {
  BlockFingerprint { content_digest: content_digest(content.as_bytes()), start, end }
}

fn previous_script_block(facts: &SfcFacts, kind: ScriptKind) -> Option<&ScriptBlockFacts> {
  facts.script.blocks.iter().find(|block| block.kind == kind)
}

fn attach_reused_summaries(
  fresh: Option<ModuleSource>,
  previous: Option<&ModuleSource>,
) -> Option<ModuleSource> {
  match (fresh, previous.and_then(ModuleSource::module_summary)) {
    (Some(module), Some(summary)) => Some(module.with_module_summary(summary)),
    (module, _) => module,
  }
}

/// Primary (`script setup` preferred) and optional ordinary dual companion.
fn dual_module_sources(
  path: &Path,
  sfc_source: &str,
  descriptor: &SfcDescriptor<'_>,
) -> (Option<ModuleSource>, Option<ModuleSource>) {
  let id = path.to_string_lossy().replace('\\', "/");
  let sfc_source = Arc::<str>::from(sfc_source);
  let setup = descriptor.script_setup.as_ref().map(|block| {
    ModuleSource::sfc_script(
      id.clone(),
      block.content.as_ref(),
      block.lang.as_deref().unwrap_or("js"),
      ScriptKind::Setup,
      block.loc.start,
      Arc::clone(&sfc_source),
    )
  });
  let ordinary = descriptor.script.as_ref().map(|block| {
    ModuleSource::sfc_script(
      if setup.is_some() { format!("{id}#script") } else { id.clone() },
      block.content.as_ref(),
      block.lang.as_deref().unwrap_or("js"),
      ScriptKind::Script,
      block.loc.start,
      Arc::clone(&sfc_source),
    )
  });
  match (setup, ordinary) {
    (Some(setup), Some(ordinary)) => (Some(setup), Some(ordinary)),
    (Some(setup), None) => (Some(setup), None),
    (None, Some(ordinary)) => (Some(ordinary), None),
    (None, None) => (None, None),
  }
}

fn merge_jsx_template_facts(target: &mut TemplateFacts, jsx: TemplateFacts) {
  if jsx.elements.is_empty() && jsx.expressions.is_empty() {
    return;
  }
  target.elements.extend(jsx.elements);
  target.expressions.extend(jsx.expressions);
  target.elements.sort_by_key(|element| element.span.offset);
  target.expressions.sort_by_key(|expression| expression.span.offset);
}

mod span;
mod style;
mod template;

#[cfg(test)]
mod tests;
