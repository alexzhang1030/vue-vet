use std::path::{Path, PathBuf};

use vue_vet_reporters::{
  FindingExplain, RuleExplain, explain_finding as build_finding_explain,
  explain_rule as build_rule_explain, looks_like_finding_id, report_diagnostic_id,
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
  if looks_like_finding_id(target) {
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
    .find(|diagnostic| report_diagnostic_id(diagnostic, &snapshot.analyzed_files) == finding_id)
  else {
    return Err(SessionError::message(format!(
      "no finding with id `{finding_id}` in the current scan; re-run with the same path that produced the id"
    )));
  };
  let file = {
    let normalized = diagnostic.file.to_string_lossy().replace('\\', "/");
    snapshot
      .analyzed_files
      .iter()
      .find(|candidate| {
        normalized == candidate.as_str()
          || normalized.strip_suffix(candidate.as_str()).is_some_and(|prefix| prefix.ends_with('/'))
      })
      .cloned()
      .unwrap_or(normalized)
  };
  let Some(meta) = resolve_rule_meta(&diagnostic.rule_id) else {
    return Err(SessionError::message(format!(
      "finding `{finding_id}` references unknown rule `{}`",
      diagnostic.rule_id
    )));
  };
  let rule = build_rule_explain(meta, &explain_search_roots(session.root()));
  Ok((build_finding_explain(finding_id, diagnostic, file, rule), snapshot.cache_status))
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
