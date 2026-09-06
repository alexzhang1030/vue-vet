//! Overlap consolidation for computed impurity findings.

use std::collections::BTreeMap;

use vue_vet_core::{Diagnostic, FileId};

const SELF_TRIGGER: &str = "vue-vet/reactivity/no-computed-self-trigger";
const SIDE_EFFECTS: &str = "vue-vet/reactivity/no-side-effects-in-computed";
const EMPTY_WATCH: &str = "vue-vet/reactivity/no-empty-watch-sources";
const UNWRAPPED_WATCH: &str = "vue-vet/reactivity/no-watch-unwrapped-source";

/// Drop the overlapping partner of the same write span after config/suppression.
///
/// Matches only this rule pair. Distinct write identities stay. The stronger
/// configured severity wins; on a tie the self-trigger finding is kept.
pub fn consolidate_overlapping_computed_impurity(diagnostics: &mut Vec<Diagnostic>) {
  struct Group {
    has_self: bool,
    has_side: bool,
    winner_rule: &'static str,
    winner_severity: vue_vet_core::Severity,
  }

  let mut groups = BTreeMap::<(FileId, usize, usize), Group>::new();
  for diagnostic in diagnostics.iter() {
    let Some(rule) = impurity_pair(diagnostic.rule_id.as_str()) else {
      continue;
    };
    let key = (diagnostic.file.clone(), diagnostic.span.offset, diagnostic.span.length);
    let group = groups.entry(key).or_insert_with(|| Group {
      has_self: false,
      has_side: false,
      winner_rule: rule,
      winner_severity: diagnostic.severity,
    });
    group.has_self |= rule == SELF_TRIGGER;
    group.has_side |= rule == SIDE_EFFECTS;
    if impurity_winner(group.winner_rule, group.winner_severity, rule, diagnostic.severity) {
      group.winner_rule = rule;
      group.winner_severity = diagnostic.severity;
    }
  }
  diagnostics.retain(|diagnostic| {
    let Some(rule) = impurity_pair(diagnostic.rule_id.as_str()) else {
      return true;
    };
    groups
      .get(&(diagnostic.file.clone(), diagnostic.span.offset, diagnostic.span.length))
      .is_none_or(|group| !(group.has_self && group.has_side) || group.winner_rule == rule)
  });
}

fn impurity_pair(rule_id: &str) -> Option<&'static str> {
  match rule_id {
    SELF_TRIGGER => Some(SELF_TRIGGER),
    SIDE_EFFECTS => Some(SIDE_EFFECTS),
    _ => None,
  }
}

fn impurity_winner(
  current_rule: &str,
  current_severity: vue_vet_core::Severity,
  candidate_rule: &str,
  candidate_severity: vue_vet_core::Severity,
) -> bool {
  candidate_severity
    .cmp(&current_severity)
    .then_with(|| impurity_specificity(candidate_rule).cmp(&impurity_specificity(current_rule)))
    .then_with(|| candidate_rule.cmp(current_rule))
    .is_gt()
}

fn impurity_specificity(rule_id: &str) -> u8 {
  match rule_id {
    SELF_TRIGGER => 1,
    _ => 0,
  }
}

/// Drop `no-empty-watch-sources` when a more specific unwrapped-source finding
/// sits inside the same `watch(...)` call.
pub fn consolidate_overlapping_watch_source_sites(diagnostics: &mut Vec<Diagnostic>) {
  let unwrapped: Vec<(FileId, usize, usize)> = diagnostics
    .iter()
    .filter(|diagnostic| diagnostic.rule_id == UNWRAPPED_WATCH)
    .map(|diagnostic| {
      (
        diagnostic.file.clone(),
        diagnostic.span.offset,
        diagnostic.span.offset.saturating_add(diagnostic.span.length),
      )
    })
    .collect();
  if unwrapped.is_empty() {
    return;
  }
  diagnostics.retain(|diagnostic| {
    if diagnostic.rule_id != EMPTY_WATCH {
      return true;
    }
    let start = diagnostic.span.offset;
    let end = diagnostic.span.offset.saturating_add(diagnostic.span.length);
    !unwrapped.iter().any(|(file, inner_start, inner_end)| {
      file == &diagnostic.file && *inner_start >= start && *inner_end <= end
    })
  });
}

#[cfg(test)]
mod tests {
  use vue_vet_core::{FileId, Severity, SourceSpan};

  use super::*;

  fn diagnostic(rule_id: &str, line: usize, offset: usize, severity: Severity) -> Diagnostic {
    Diagnostic {
      rule_id: rule_id.into(),
      category: "reactivity".into(),
      severity,
      confidence: None,
      documentation: None,
      message: format!("{rule_id}:{line}"),
      help: None,
      file: FileId::from("ComputedCauses.vue"),
      span: SourceSpan { offset, length: 8, line, column: 1 },
      edits: Vec::new(),
      recommendation: None,
    }
  }

  #[test]
  fn keeps_one_finding_per_overlapping_write_and_retains_distinct_writes() {
    let mut diagnostics = vec![
      diagnostic(SELF_TRIGGER, 6, 10, Severity::Warning),
      diagnostic(SIDE_EFFECTS, 6, 10, Severity::Warning),
      diagnostic(SELF_TRIGGER, 7, 20, Severity::Warning),
      diagnostic(SIDE_EFFECTS, 7, 20, Severity::Warning),
      diagnostic(SELF_TRIGGER, 7, 40, Severity::Warning),
      diagnostic(SIDE_EFFECTS, 7, 40, Severity::Warning),
    ];
    consolidate_overlapping_computed_impurity(&mut diagnostics);
    assert_eq!(diagnostics.len(), 3);
    assert!(diagnostics.iter().all(|row| row.rule_id == SELF_TRIGGER));
    assert_eq!(diagnostics.iter().map(|row| row.span.line).collect::<Vec<_>>(), vec![6, 7, 7]);
  }

  #[test]
  fn keeps_stronger_severity_and_leaves_a_disabled_partner() {
    let mut diagnostics = vec![
      diagnostic(SELF_TRIGGER, 6, 10, Severity::Warning),
      diagnostic(SIDE_EFFECTS, 6, 10, Severity::Error),
    ];
    consolidate_overlapping_computed_impurity(&mut diagnostics);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
      diagnostics.first().map(|row| (row.rule_id.as_str(), row.severity)),
      Some((SIDE_EFFECTS, Severity::Error))
    );

    let mut only_self = vec![diagnostic(SELF_TRIGGER, 6, 10, Severity::Warning)];
    consolidate_overlapping_computed_impurity(&mut only_self);
    assert_eq!(only_self.len(), 1);
    assert_eq!(only_self.first().map(|row| row.rule_id.as_str()), Some(SELF_TRIGGER));
  }

  #[test]
  fn suppressed_partner_is_left_alone() {
    let mut diagnostics = vec![
      diagnostic(SELF_TRIGGER, 6, 10, Severity::Warning),
      diagnostic("vue-vet/reactivity/unrelated", 6, 10, Severity::Warning),
    ];
    consolidate_overlapping_computed_impurity(&mut diagnostics);
    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics.first().map(|row| row.rule_id.as_str()), Some(SELF_TRIGGER));
  }

  #[test]
  fn drops_empty_watch_when_unwrapped_source_is_inside_the_call() {
    let mut diagnostics = vec![
      Diagnostic {
        rule_id: EMPTY_WATCH.into(),
        category: "reactivity".into(),
        severity: Severity::Warning,
        confidence: None,
        documentation: None,
        message: "empty".into(),
        help: None,
        file: FileId::from("Watch.vue"),
        span: SourceSpan { offset: 10, length: 20, line: 4, column: 1 },
        edits: Vec::new(),
        recommendation: None,
      },
      Diagnostic {
        rule_id: UNWRAPPED_WATCH.into(),
        category: "reactivity".into(),
        severity: Severity::Warning,
        confidence: None,
        documentation: None,
        message: "unwrapped".into(),
        help: None,
        file: FileId::from("Watch.vue"),
        span: SourceSpan { offset: 16, length: 7, line: 4, column: 7 },
        edits: Vec::new(),
        recommendation: None,
      },
    ];
    consolidate_overlapping_watch_source_sites(&mut diagnostics);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics.first().map(|row| row.rule_id.as_str()), Some(UNWRAPPED_WATCH));
  }
}
