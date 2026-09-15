//! Diagnostics, spans, scoring summary, and explain payloads.

use std::fmt::Write;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::edits::TextEdit;
use crate::groups::RuleGroupId;
use crate::identity::FileId;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
  Info,
  Warning,
  Error,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
  High,
  Medium,
  Low,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct RuleMeta {
  pub id: &'static str,
  pub category: &'static str,
  pub default_severity: Severity,
  pub confidence: Confidence,
  pub documentation: &'static str,
  /// Canonical product group. `None` for unmapped IDs (a11y, template parity, …).
  #[serde(skip_serializing_if = "Option::is_none")]
  pub group: Option<RuleGroupId>,
}

/// Byte offset plus derived line/column. Four `usize`s; pass by copy.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct SourceSpan {
  pub offset: usize,
  pub length: usize,
  pub line: usize,
  pub column: usize,
}

impl Default for SourceSpan {
  fn default() -> Self {
    Self { offset: 0, length: 0, line: 1, column: 1 }
  }
}

/// Category for ecosystem / best-practice suggestions (excluded from score and CI exit).
pub const PRACTICE_CATEGORY: &str = "practice";

/// Category for opt-in migration assessment (excluded from score and CI exit).
pub const MIGRATION_CATEGORY: &str = "migration";

/// Optional ecosystem API recommendation attached to a finding (JSON v1 additive).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Recommendation {
  /// Recommendation shape; currently always `ecosystem_api`.
  pub kind: String,
  pub package: String,
  pub export: String,
  pub docs_url: String,
  pub import_example: String,
}

/// Opt-in source of a Vapor migration assessment.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OptIn {
  ScriptVaporAttr,
  TemplateVaporAttr,
  None,
}

/// Assessment check / aggregate verdict.
///
/// `ready` is the aggregate-only “recommended for direct conversion” answer.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
  CompilerCandidate,
  Blocked,
  NeedsVerification,
  Unsupported,
  NotApplicable,
  Ready,
}

impl Verdict {
  /// Severity rank for aggregate: blocked > unsupported > needs-verification > compiler-candidate.
  /// `not-applicable` never wins. `ready` ranks with `compiler-candidate` and is
  /// only assigned by [`Assessment::aggregate_verdict`].
  #[must_use]
  pub const fn rank(self) -> u8 {
    match self {
      Self::Blocked => 4,
      Self::Unsupported => 3,
      Self::NeedsVerification => 2,
      Self::CompilerCandidate | Self::Ready => 1,
      Self::NotApplicable => 0,
    }
  }
}

/// Can this component be converted to Vapor at all?
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Convertible {
  Yes,
  No,
  Unknown,
}

/// One named check inside an [`Assessment`].
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AssessmentCheck {
  pub check: String,
  pub verdict: Verdict,
  pub reasons: Vec<String>,
}

/// Vue Vet-owned migration assessment record (JSON v1 additive).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Assessment {
  pub kind: String,
  pub opt_in: OptIn,
  pub complete: bool,
  pub unknown: Vec<String>,
  pub checks: Vec<AssessmentCheck>,
  pub aggregate: Verdict,
  pub convertible: Convertible,
}

impl Assessment {
  /// Worst applicable check verdict. `not-applicable` never wins.
  /// Incomplete assessments cannot be better than `needs-verification`.
  /// `ready` only when `complete`, no check is `blocked` / `unsupported` /
  /// `needs-verification`, and `runtime-envelope` is `compiler-candidate`.
  #[must_use]
  pub fn aggregate_verdict(checks: &[AssessmentCheck], complete: bool) -> Verdict {
    let mut best = Verdict::NotApplicable;
    for check in checks {
      if check.verdict.rank() > best.rank() {
        best = check.verdict;
      }
    }
    if !complete && best.rank() < Verdict::NeedsVerification.rank() {
      return Verdict::NeedsVerification;
    }
    let envelope_ready = checks.iter().any(|check| {
      check.check == "runtime-envelope" && check.verdict == Verdict::CompilerCandidate
    });
    if complete
      && envelope_ready
      && !checks.iter().any(|check| {
        matches!(
          check.verdict,
          Verdict::Blocked | Verdict::Unsupported | Verdict::NeedsVerification
        )
      })
    {
      return Verdict::Ready;
    }
    best
  }

  /// Answer (1): can this component be converted to Vapor at all?
  #[must_use]
  pub fn convertible_of(checks: &[AssessmentCheck], complete: bool) -> Convertible {
    if checks.iter().any(|check| matches!(check.verdict, Verdict::Blocked | Verdict::Unsupported)) {
      Convertible::No
    } else if !complete {
      Convertible::Unknown
    } else {
      Convertible::Yes
    }
  }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Diagnostic {
  pub rule_id: String,
  pub category: String,
  pub severity: Severity,
  pub confidence: Option<Confidence>,
  pub documentation: Option<String>,
  pub message: String,
  pub help: Option<String>,
  pub file: FileId,
  pub span: SourceSpan,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub edits: Vec<TextEdit>,
  /// Ecosystem or official-API suggestion payload (practice findings).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub recommendation: Option<Recommendation>,
  /// Opt-in migration assessment payload (migration findings).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub assessment: Option<Assessment>,
}

impl Diagnostic {
  /// Practice and migration findings do not affect score or default CI failure.
  #[must_use]
  pub fn affects_score(&self) -> bool {
    self.category != PRACTICE_CATEGORY && self.category != MIGRATION_CATEGORY
  }

  /// Practice and migration findings never fail the scan unless later opt-in policy says so.
  #[must_use]
  pub fn affects_exit(&self) -> bool {
    self.category != PRACTICE_CATEGORY && self.category != MIGRATION_CATEGORY
  }
}

/// Domain payload for explaining one rule. Reporters only render this model.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RuleExplain {
  pub rule_id: String,
  pub category: String,
  pub severity: Severity,
  pub confidence: Confidence,
  pub documentation: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub body: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub body_path: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub body_error: Option<String>,
}

/// Domain payload for explaining one concrete finding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FindingExplain {
  pub id: String,
  pub file: String,
  pub span: SourceSpan,
  pub severity: Severity,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub confidence: Option<Confidence>,
  pub message: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub help: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub recommendation: Option<Recommendation>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub assessment: Option<Assessment>,
  pub rule: RuleExplain,
  /// When the finding sits on a tracking scope, static “would Vue re-run?” evidence.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub tracking: Option<ScopeExplain>,
}

/// Why a scope tracks (or does not track) a dependency — multi-consumer contract.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeTrackReason {
  /// Read always runs while the scope tracks.
  Unconditional,
  /// Read only on some control-flow paths.
  Conditional,
  /// Read after `await` (effect stopped tracking).
  AfterAwait,
  /// Read outside tracking (then/nextTick/watch callback/deferred).
  OutsideTracking,
  /// Soft evidence: unclassified `.value` / `unref` / `toValue` root.
  UncertainAccess,
  /// Scope has no known reactive reads (absence rules fire).
  NoKnownDependency,
}

/// One dependency line in a [`ScopeExplain`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ScopeExplainDep {
  pub binding: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub property: Option<String>,
  /// Display path (`count.value`, `useI18n@12.locale`, bare `props`).
  pub path: String,
  pub reason: ScopeTrackReason,
  /// Short human label for `reason`.
  pub reason_label: String,
  pub span: SourceSpan,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub guards: Vec<String>,
}

/// Static explanation of what a tracking scope depends on (and what it does not).
///
/// Killer product surface: CI / CLI / editor “would Vue re-run this?” without `DevTools`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ScopeExplain {
  pub module_id: String,
  pub kind: String,
  pub callee: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub binding: Option<String>,
  pub span: SourceSpan,
  /// One-line verdict for humans and agents.
  pub summary: String,
  /// Known dependencies that participate in tracking (unconditional + conditional).
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub tracks: Vec<ScopeExplainDep>,
  /// Reads that do **not** establish tracking (after-await, outside, …).
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub does_not_track: Vec<ScopeExplainDep>,
  /// Soft roots (`maybe:`) that were not classified as known bindings.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub uncertain: Vec<String>,
  /// Identifier / member callees the bounded tracer did not follow.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub unknown_calls: Vec<String>,
  /// Helper follow stopped at the depth cap or a recursive callee.
  #[serde(default, skip_serializing_if = "explain_flag_is_false")]
  pub follow_truncated: bool,
  /// False when `unknown_calls`, `follow_truncated`, or `uncertain` is set.
  /// Omitted when true.
  #[serde(skip_serializing_if = "explain_flag_is_true")]
  pub analysis_complete: bool,
}

/// Builds the stable, opaque identity used by machine-readable report consumers.
///
/// The caller supplies a repository-relative path because only the orchestration
/// layer knows the scan root. The content digest changes with user-visible
/// severity or message changes while the readable prefix keeps triage practical.
#[must_use]
pub fn diagnostic_id(diagnostic: &Diagnostic, normalized_file_path: &str) -> String {
  let mut hasher = Sha256::new();
  let severity = match diagnostic.severity {
    Severity::Info => "info",
    Severity::Warning => "warning",
    Severity::Error => "error",
  };
  hash_identity_field(&mut hasher, b"severity", severity.as_bytes());
  hash_identity_field(&mut hasher, b"message", diagnostic.message.as_bytes());
  let digest = hex_digest(&hasher.finalize());
  format!(
    "{normalized_file_path}::{}:{}::{}::{digest}",
    diagnostic.span.line, diagnostic.span.column, diagnostic.rule_id
  )
}

/// Stable finding identity using the diagnostic's normalized [`FileId`].
#[must_use]
pub fn finding_id(diagnostic: &Diagnostic) -> String {
  diagnostic_id(diagnostic, diagnostic.file.as_str())
}

#[expect(clippy::trivially_copy_pass_by_ref, reason = "serde skip_serializing_if takes &T")]
const fn explain_flag_is_false(value: &bool) -> bool {
  !*value
}

#[expect(clippy::trivially_copy_pass_by_ref, reason = "serde skip_serializing_if takes &T")]
const fn explain_flag_is_true(value: &bool) -> bool {
  *value
}

fn hash_identity_field(hasher: &mut Sha256, name: &[u8], value: &[u8]) {
  let name_length = u64::try_from(name.len()).unwrap_or(u64::MAX);
  let value_length = u64::try_from(value.len()).unwrap_or(u64::MAX);
  hasher.update(name_length.to_le_bytes());
  hasher.update(name);
  hasher.update(value_length.to_le_bytes());
  hasher.update(value);
}

fn hex_digest(bytes: &[u8]) -> String {
  let mut output = String::with_capacity(bytes.len().saturating_mul(2));
  for byte in bytes {
    if write!(&mut output, "{byte:02x}").is_err() {
      break;
    }
  }
  output
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScanSummary {
  pub files_scanned: usize,
  pub diagnostics: Vec<Diagnostic>,
  pub score: u8,
}

impl ScanSummary {
  #[must_use]
  pub fn finish(mut self) -> Self {
    self.diagnostics.sort_by(|left, right| {
      (&left.file, left.span.offset, &left.rule_id).cmp(&(
        &right.file,
        right.span.offset,
        &right.rule_id,
      ))
    });

    let raw_weight = self.diagnostics.iter().filter(|diagnostic| diagnostic.affects_score()).fold(
      0_u32,
      |total, diagnostic| {
        total.saturating_add(match diagnostic.severity {
          Severity::Error => 10,
          Severity::Warning => 3,
          Severity::Info => 1,
        })
      },
    );
    self.score = density_score(raw_weight, self.files_scanned);
    self
  }

  #[must_use]
  pub fn fails(&self, deny_warnings: bool) -> bool {
    self.diagnostics.iter().filter(|diagnostic| diagnostic.affects_exit()).any(|diagnostic| {
      diagnostic.severity == Severity::Error
        || (deny_warnings && diagnostic.severity == Severity::Warning)
    })
  }
}

/// Maps severity weights to a 0–100 score by finding density, not absolute count.
///
/// Industry health scores (`SonarQube` / `CodeClimate` technical-debt ratio,
/// `StackHealth` lint density) normalize by codebase size so a large Nuxt app
/// with sparse warnings is not punished like a tiny project with the same
/// absolute count. Vue Vet uses scanned files as the size proxy:
/// `score = floor(100 × capacity / (capacity + raw))` where
/// `capacity = max(files, 1) × 50` and raw uses Error 10 / Warning 3 / Info 1.
#[must_use]
pub fn density_score(raw_weight: u32, files_scanned: usize) -> u8 {
  const FILE_BUDGET: u32 = 50;
  if raw_weight == 0 {
    return 100;
  }
  let files = u32::try_from(files_scanned.max(1)).unwrap_or(1);
  let capacity = files.saturating_mul(FILE_BUDGET);
  let score = (100_u32.saturating_mul(capacity)) / capacity.saturating_add(raw_weight);
  u8::try_from(score.min(100)).unwrap_or(0)
}
