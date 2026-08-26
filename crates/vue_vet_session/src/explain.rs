use std::{
  fs,
  path::{Path, PathBuf},
};

use vue_vet_core::{
  Diagnostic, FindingExplain, RuleExplain, RuleMeta, ScopeExplain,
  finding_id as diagnostic_finding_id,
};
use vue_vet_reactivity::{
  explain_tracking_scope, module_id_matches, query_module_prefix, scope_covering_span,
  select_tracking_scopes,
};

use crate::{ProjectSession, SessionError, registry::resolve_rule_meta};

/// Session explain payload for text/JSON rendering by the caller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Explained {
  Rule(RuleExplain),
  Finding { explain: Box<FindingExplain>, cache_status: &'static str },
}

pub fn explain(session: &ProjectSession, target: &str) -> Result<Explained, SessionError> {
  if resolve_rule_meta(target).is_some() {
    return Ok(Explained::Rule(explain_rule(session, target)?));
  }
  if target.contains("::") {
    let (explain, cache_status) = explain_finding_with_status(session, target)?;
    return Ok(Explained::Finding { explain: Box::new(explain), cache_status });
  }
  Err(SessionError::message(format!(
    "unknown rule `{target}`; pass a full rule id such as `vue-vet/security/no-v-html`, or a finding id from `--format json`"
  )))
}

pub fn explain_rule(session: &ProjectSession, rule_id: &str) -> Result<RuleExplain, SessionError> {
  let Some(meta) = resolve_rule_meta(rule_id) else {
    return Err(SessionError::message(format!(
      "unknown rule `{rule_id}`; pass a full rule id such as `vue-vet/security/no-v-html`"
    )));
  };
  Ok(build_rule_explain(meta, &explain_search_roots(session.root())))
}

pub fn explain_finding(
  session: &ProjectSession,
  finding_id: &str,
) -> Result<FindingExplain, SessionError> {
  Ok(explain_finding_with_status(session, finding_id)?.0)
}

/// Static “would Vue re-run this?” for tracking scopes matching `query`.
///
/// Query forms (same as [`select_tracking_scopes`]):
/// - binding name (`label`, `doubled`)
/// - `module:binding` or `module:` (all scopes in matching modules)
/// - `@offset` / `callee@offset` / `module@offset` (`@offset` also covers a caret inside a span)
pub fn explain_scope(
  session: &ProjectSession,
  query: &str,
) -> Result<(Vec<ScopeExplain>, &'static str), SessionError> {
  let query = query.trim();
  if query.is_empty() {
    return Err(SessionError::message(
      "empty --explain-scope query; pass a binding name, `file:binding`, or `@offset`",
    ));
  }
  let snapshot = snapshot_for_explain(session)?;
  let mut explains = collect_scope_explains(&snapshot.graph.module_reactivity, query);
  if explains.is_empty() {
    return Err(SessionError::message(format!(
      "no tracking scope matched `{query}`; try a binding name (e.g. `label`), `file.vue:label`, or `@offset` from `--print-reactivity`"
    )));
  }
  explains.sort_by(|left, right| {
    (left.module_id.as_str(), left.span.offset, left.kind.as_str(), left.callee.as_str()).cmp(&(
      right.module_id.as_str(),
      right.span.offset,
      right.kind.as_str(),
      right.callee.as_str(),
    ))
  });
  Ok((explains, snapshot.cache_status))
}

fn explain_finding_with_status(
  session: &ProjectSession,
  finding_id: &str,
) -> Result<(FindingExplain, &'static str), SessionError> {
  let snapshot = snapshot_for_explain(session)?;
  let Some(diagnostic) = snapshot
    .summary
    .diagnostics
    .iter()
    .find(|diagnostic| diagnostic_finding_id(diagnostic) == finding_id)
  else {
    return Err(SessionError::message(format!(
      "no finding with id `{finding_id}` in the current scan; re-run with the same path that produced the id"
    )));
  };
  let file = diagnostic.file.as_str().to_owned();
  let Some(meta) = resolve_rule_meta(&diagnostic.rule_id) else {
    return Err(SessionError::message(format!(
      "finding `{finding_id}` references unknown rule `{}`",
      diagnostic.rule_id
    )));
  };
  let rule = build_rule_explain(meta, &explain_search_roots(session.root()));
  let mut explain = build_finding_explain(finding_id, diagnostic, file, rule);
  explain.tracking = tracking_for_diagnostic(&snapshot.graph.module_reactivity, diagnostic);
  Ok((explain, snapshot.cache_status))
}

fn tracking_for_diagnostic(
  modules: &[vue_vet_reactivity::ModuleReactivity],
  diagnostic: &Diagnostic,
) -> Option<ScopeExplain> {
  let module_id = diagnostic.file.as_str();
  let module = modules.iter().find(|module| module_id_matches(module.id.as_str(), module_id))?;
  let scope =
    scope_covering_span(module.graph.as_ref(), diagnostic.span.offset, diagnostic.span.length)?;
  Some(explain_tracking_scope(module.id.as_str(), scope))
}

fn collect_scope_explains(
  modules: &[vue_vet_reactivity::ModuleReactivity],
  query: &str,
) -> Vec<ScopeExplain> {
  let query_module = query_module_prefix(query);
  let mut explains = Vec::new();
  for module in modules {
    let module_id = module.id.as_str();
    if query_module.is_some_and(|prefix| !module_id_matches(module_id, prefix)) {
      continue;
    }
    let selected = select_tracking_scopes(module_id, module.graph.as_ref(), query);
    if selected.is_empty() {
      // `module:` or bare module path → every scope in matching modules.
      if module_list_query(module_id, query) {
        for scope in &module.graph.scopes {
          explains.push(explain_tracking_scope(module_id, scope));
        }
      }
      continue;
    }
    for scope in selected {
      explains.push(explain_tracking_scope(module_id, scope));
    }
  }
  explains
}

/// `App.vue`, `App.vue:`, or trailing `path/App.vue:` lists all scopes in that module.
fn module_list_query(module_id: &str, query: &str) -> bool {
  let trimmed = query.strip_suffix(':').unwrap_or(query);
  if trimmed.is_empty() || trimmed.contains('@') {
    return false;
  }
  // Only treat as module list when the query is path-like or ends with a known extension,
  // or explicitly used the `module:` form (empty binding after `:`).
  let explicit_module_list = query.ends_with(':');
  let path_like = trimmed.contains('/')
    || trimmed.contains('\\')
    || Path::new(trimmed).extension().is_some_and(|ext| {
      ext.eq_ignore_ascii_case("vue")
        || ext.eq_ignore_ascii_case("ts")
        || ext.eq_ignore_ascii_case("tsx")
        || ext.eq_ignore_ascii_case("js")
        || ext.eq_ignore_ascii_case("jsx")
        || ext.eq_ignore_ascii_case("mts")
        || ext.eq_ignore_ascii_case("cts")
    });
  if !(explicit_module_list || path_like) {
    return false;
  }
  module_id_matches(module_id, trimmed)
}

fn build_rule_explain(meta: &RuleMeta, search_roots: &[PathBuf]) -> RuleExplain {
  let documentation = format!("docs/{}.md", meta.documentation);
  let (body, body_path, body_error) =
    match load_documentation_body(meta.documentation, search_roots) {
      Ok((path, text)) => (Some(text), Some(path.display().to_string().replace('\\', "/")), None),
      Err(error) => (None, None, Some(error)),
    };
  RuleExplain {
    rule_id: meta.id.into(),
    category: meta.category.into(),
    severity: meta.default_severity,
    confidence: meta.confidence,
    documentation,
    body,
    body_path,
    body_error,
  }
}

fn build_finding_explain(
  id: impl Into<String>,
  diagnostic: &Diagnostic,
  file: impl Into<String>,
  rule: RuleExplain,
) -> FindingExplain {
  FindingExplain {
    id: id.into(),
    file: file.into(),
    span: diagnostic.span.clone(),
    severity: diagnostic.severity,
    confidence: diagnostic.confidence,
    message: diagnostic.message.clone(),
    help: diagnostic.help.clone(),
    recommendation: diagnostic.recommendation.clone(),
    rule,
    tracking: None,
  }
}

fn load_documentation_body(
  documentation_key: &str,
  search_roots: &[PathBuf],
) -> Result<(PathBuf, String), String> {
  let relative = format!("docs/{documentation_key}.md");
  let mut tried = Vec::new();
  for root in search_roots {
    for candidate in documentation_candidates(root, &relative) {
      tried.push(candidate.display().to_string());
      match fs::read_to_string(&candidate) {
        Ok(text) => return Ok((candidate, text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("failed to read {}: {error}", candidate.display())),
      }
    }
  }
  Err(format!(
    "could not find {relative} under {}; install from source or clone the Vue Vet repository to read full rule docs",
    if tried.is_empty() { "the search roots".into() } else { tried.join(", ") }
  ))
}

fn documentation_candidates(root: &Path, relative_docs_path: &str) -> Vec<PathBuf> {
  let mut candidates = vec![root.join(relative_docs_path)];
  let mut current = root.to_path_buf();
  for _ in 0..8 {
    let Some(parent) = current.parent().map(Path::to_path_buf) else {
      break;
    };
    if parent == current {
      break;
    }
    candidates.push(parent.join(relative_docs_path));
    current = parent;
  }
  candidates
}

/// Prefer the last committed full snapshot so LSP hover / MCP explain-scope
/// can reuse a `DiagnosticsOnly` publish without re-tracing.
fn snapshot_for_explain(session: &ProjectSession) -> Result<crate::AnalysisSnapshot, SessionError> {
  session.current_snapshot()?.map_or_else(|| session.analyze(), |snapshot| Ok((*snapshot).clone()))
}

fn explain_search_roots(scan_path: &Path) -> Vec<PathBuf> {
  let mut roots = Vec::new();
  let scan = if scan_path.is_file() {
    scan_path.parent().unwrap_or(scan_path).to_path_buf()
  } else {
    scan_path.to_path_buf()
  };
  roots.push(scan);
  if let Ok(cwd) = std::env::current_dir()
    && !roots.iter().any(|root| root == &cwd)
  {
    roots.push(cwd);
  }
  roots
}
