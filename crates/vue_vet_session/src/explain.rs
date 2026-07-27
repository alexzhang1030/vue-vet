use std::{
  fs,
  path::{Path, PathBuf},
};
use vue_vet_core::{
  Diagnostic, FindingExplain, RuleExplain, RuleMeta, finding_id as diagnostic_finding_id,
};

use crate::{ProjectSession, SessionError, resolve_rule_meta};

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

fn explain_finding_with_status(
  session: &ProjectSession,
  finding_id: &str,
) -> Result<(FindingExplain, &'static str), SessionError> {
  let snapshot = session.analyze()?;
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
  Ok((build_finding_explain(finding_id, diagnostic, file, rule), snapshot.cache_status))
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
