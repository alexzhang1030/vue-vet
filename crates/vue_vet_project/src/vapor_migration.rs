//! Opt-in Vapor component-migration assessment (issue #247).
//!
//! Pure functions from project facts + graph → diagnostics. Off-score, Info.

use std::{
  collections::{BTreeMap, BTreeSet},
  fs,
  path::Path,
};

use vue_vet_core::{
  Assessment, AssessmentCheck, Confidence, Convertible, Diagnostic, MIGRATION_CATEGORY, OptIn,
  ScriptCallFact, ScriptKind, SourceSpan, Verdict, VueVersion,
};

use crate::context::ProjectContext;
use crate::model::{EdgeKind, GraphEdge, GraphNode, ProjectFile};
use crate::resolve::normalized_path;

/// `vue` / `@vue/compiler-sfc` / `@vue/compiler-vapor` / `@vue/runtime-vapor`.
/// Audited identity: vuejs/core `4b2f1914e8a6da7218955593b8bc2ba5db2c6dce`.
pub const AUDITED_VUE_VERSION: &str = "3.6.0-rc.7";
/// `@vitejs/plugin-vue`. Audited identity: `d8ff7d0e8f557a7c1975c07b30e232c69bdbbc03`.
pub const AUDITED_PLUGIN_VUE_VERSION: &str = "6.0.8";
pub const AUDITED_CORE_COMMIT: &str = "4b2f1914e8a6da7218955593b8bc2ba5db2c6dce";
pub const AUDITED_PLUGIN_VUE_COMMIT: &str = "d8ff7d0e8f557a7c1975c07b30e232c69bdbbc03";

pub const VAPOR_MIGRATION_RULE_IDS: [&str; 5] = [
  "vue-vet/migration/vapor-assessment",
  "vue-vet/migration/vapor-interop-required",
  "vue-vet/migration/vapor-memo-contract-dropped",
  "vue-vet/migration/vapor-runtime-envelope",
  "vue-vet/migration/vapor-sfc-compile-contract",
];

const ASSESSMENT_ID: &str = "vue-vet/migration/vapor-assessment";
const INTEROP_ID: &str = "vue-vet/migration/vapor-interop-required";
const MEMO_ID: &str = "vue-vet/migration/vapor-memo-contract-dropped";
const ENVELOPE_ID: &str = "vue-vet/migration/vapor-runtime-envelope";
const SFC_ID: &str = "vue-vet/migration/vapor-sfc-compile-contract";

const TOOLCHAIN_PACKAGES: &[(&str, &str)] = &[
  ("vue", AUDITED_VUE_VERSION),
  ("@vue/compiler-sfc", AUDITED_VUE_VERSION),
  ("@vue/compiler-vapor", AUDITED_VUE_VERSION),
  ("@vue/runtime-vapor", AUDITED_VUE_VERSION),
  ("@vitejs/plugin-vue", AUDITED_PLUGIN_VUE_VERSION),
];

const INTEROP_BUILTINS: &[&str] =
  &["suspense", "teleport", "keepalive", "transition", "transitiongroup"];

/// Admitted Vapor runtime envelope from
/// [`research/vapor-migration/README.md`](../../../research/vapor-migration/README.md)
/// (“Admitted envelope”). The oracle executed `ref`, text interpolation,
/// `v-if` / `v-else` / `v-else-if`, keyed `v-for` on a ref array, click
/// handlers without modifiers, `v-once`, `v-memo` (blocked separately), and
/// `v-bind` / `:attr`. Widening this list requires first adding and passing a
/// runtime oracle fixture pair there. Any modifier-free native `v-on` is treated
/// as inside the envelope — the only generalization beyond the executed click.
const ENVELOPE_VUE_BUILT_IN_DIRECTIVES: &[&str] = &[
  "if", "else", "else-if", "for", "on", "bind", "model", "slot", "pre", "once", "memo", "cloak",
  "show", "html", "text", "is",
];
const ENVELOPE_UNTESTED_BUILT_IN_DIRECTIVES: &[&str] = &["model", "show", "html", "text", "slot"];
const ENVELOPE_SCRIPT_APIS: &[&str] = &[
  "provide",
  "inject",
  "defineModel",
  "defineSlots",
  "useSlots",
  "useAttrs",
  "defineExpose",
  "getCurrentInstance",
  "h",
  "render",
];

/// Emit assessment + per-reason diagnostics for every `.vue` file.
#[must_use]
pub fn vapor_migration_diagnostics(
  root: &Path,
  files: &[&ProjectFile],
  nodes: &[GraphNode],
  edges: &[GraphEdge],
  project_context: &ProjectContext,
) -> Vec<Diagnostic> {
  let toolchain = classify_toolchain(root);
  let ssr = classify_ssr(files, project_context);
  let children = ChildIndex::new(files, nodes, edges);
  let mut diagnostics = Vec::new();
  for file in files {
    if !is_vue_file(file.path.as_path()) {
      continue;
    }
    diagnostics.extend(assess_file(file, &children, &toolchain, &ssr));
  }
  diagnostics
}

/// Project-wide lookups built once so each file's interop check is O(its template).
struct ChildIndex<'a> {
  /// `file:<path>` → component-usage edges leaving that file.
  outgoing: BTreeMap<&'a str, Vec<&'a GraphEdge>>,
  node_by_id: BTreeMap<&'a str, &'a GraphNode>,
  vapor_by_path: BTreeMap<String, bool>,
}

impl<'a> ChildIndex<'a> {
  fn new(files: &[&'a ProjectFile], nodes: &'a [GraphNode], edges: &'a [GraphEdge]) -> Self {
    let mut outgoing: BTreeMap<&str, Vec<&GraphEdge>> = BTreeMap::new();
    for edge in edges {
      if matches!(edge.kind, EdgeKind::ComponentUsage | EdgeKind::AutoComponent) {
        outgoing.entry(edge.from.as_str()).or_default().push(edge);
      }
    }
    Self {
      outgoing,
      node_by_id: nodes.iter().map(|node| (node.id.as_str(), node)).collect(),
      vapor_by_path: files
        .iter()
        .map(|file| (normalized_path(file.path.as_path()), is_vapor_opted_in(file)))
        .collect(),
    }
  }

  fn resolve(&self, from: &str, tag: &str) -> ChildResolve {
    let key = comparable_name(tag);
    let Some(edge) = self
      .outgoing
      .get(from)
      .and_then(|edges| edges.iter().find(|edge| comparable_name(&edge.specifier) == key))
    else {
      return ChildResolve::Unresolved;
    };
    let Some(node) = self.node_by_id.get(edge.to.as_str()) else {
      return ChildResolve::Unresolved;
    };
    match self.vapor_by_path.get(&node.path) {
      Some(true) => ChildResolve::Vapor,
      Some(false) | None => ChildResolve::NonVapor,
    }
  }
}

struct CheckOutcome {
  verdict: Verdict,
  reasons: Vec<String>,
  unknown: Vec<String>,
}

struct LocatedReason {
  reason: String,
  span: SourceSpan,
}

fn assess_file(
  file: &ProjectFile,
  children: &ChildIndex<'_>,
  toolchain: &CheckOutcome,
  ssr: &CheckOutcome,
) -> Vec<Diagnostic> {
  let sfc = sfc_compile_contract(file);
  let memo = memo_contract(file);
  let interop = interop_required(file, children);
  let envelope = runtime_envelope(file);
  let mut unknown = BTreeSet::new();
  for extra in [
    &toolchain.unknown,
    &ssr.unknown,
    &sfc.outcome.unknown,
    &memo.outcome.unknown,
    &interop.outcome.unknown,
    &envelope.outcome.unknown,
  ] {
    unknown.extend(extra.iter().cloned());
  }
  if has_unaudited_dependency_closure(file) {
    unknown.insert("dependency closure unaudited".into());
  }
  let unknown = unknown.into_iter().collect::<Vec<_>>();
  let complete = unknown.is_empty();
  let checks = vec![
    assessment_check("toolchain-tuple", toolchain),
    assessment_check("sfc-compile-contract", &sfc.outcome),
    assessment_check("memo-contract-dropped", &memo.outcome),
    assessment_check("interop-required", &interop.outcome),
    assessment_check("ssr-hydration", ssr),
    assessment_check("runtime-envelope", &envelope.outcome),
  ];
  let aggregate = Assessment::aggregate_verdict(&checks, complete);
  let convertible = Assessment::convertible_of(&checks, complete);
  let opt_in = opt_in_of(file);
  let assessment = Assessment {
    kind: "vapor-migration".into(),
    opt_in,
    complete,
    unknown,
    checks,
    aggregate,
    convertible,
  };
  let span = assessment_span(file);
  let mut diagnostics = vec![migration_diagnostic(
    ASSESSMENT_ID,
    "rules/migration/vapor-assessment",
    file,
    span,
    assessment_message(&assessment),
    Some(assessment),
  )];
  for located in sfc.findings {
    diagnostics.push(migration_diagnostic(
      SFC_ID,
      "rules/migration/vapor-sfc-compile-contract",
      file,
      located.span,
      located.reason,
      None,
    ));
  }
  for located in memo.findings {
    diagnostics.push(migration_diagnostic(
      MEMO_ID,
      "rules/migration/vapor-memo-contract-dropped",
      file,
      located.span,
      located.reason,
      None,
    ));
  }
  for located in interop.findings {
    diagnostics.push(migration_diagnostic(
      INTEROP_ID,
      "rules/migration/vapor-interop-required",
      file,
      located.span,
      located.reason,
      None,
    ));
  }
  for located in envelope.findings {
    diagnostics.push(migration_diagnostic(
      ENVELOPE_ID,
      "rules/migration/vapor-runtime-envelope",
      file,
      located.span,
      located.reason,
      None,
    ));
  }
  diagnostics
}

fn assessment_check(name: &str, outcome: &CheckOutcome) -> AssessmentCheck {
  AssessmentCheck { check: name.into(), verdict: outcome.verdict, reasons: outcome.reasons.clone() }
}

fn assessment_message(assessment: &Assessment) -> String {
  match (assessment.convertible, assessment.aggregate) {
    (_, Verdict::Ready) => {
      "vapor migration: ready — direct conversion recommended (all constructs inside the verified runtime envelope)".into()
    }
    (Convertible::Yes, _) => {
      format!(
        "vapor migration: convertible, verification pending ({})",
        pending_preview(&assessment.checks)
      )
    }
    (Convertible::No, aggregate) => {
      format!(
        "vapor migration: not convertible as-is — {} ({})",
        verdict_slug(aggregate),
        first_blocking_check(&assessment.checks)
      )
    }
    (Convertible::Unknown, _) => {
      format!("vapor migration: unknown — {}", unknown_preview(&assessment.unknown))
    }
  }
}

fn pending_preview(checks: &[AssessmentCheck]) -> String {
  let Some(check) = checks.iter().find(|check| check.verdict == Verdict::NeedsVerification) else {
    return "verification pending".into();
  };
  if check.reasons.is_empty() {
    return check.check.clone();
  }
  let reasons = check.reasons.iter().take(3).map(String::as_str).collect::<Vec<_>>().join(", ");
  format!("{}: {reasons}", check.check)
}

fn first_blocking_check(checks: &[AssessmentCheck]) -> String {
  checks
    .iter()
    .find(|check| matches!(check.verdict, Verdict::Blocked | Verdict::Unsupported))
    .map_or_else(|| "blocked".into(), |check| check.check.clone())
}

fn unknown_preview(unknown: &[String]) -> String {
  if unknown.is_empty() {
    return "incomplete".into();
  }
  unknown.iter().take(3).map(String::as_str).collect::<Vec<_>>().join(", ")
}

const fn verdict_slug(verdict: Verdict) -> &'static str {
  match verdict {
    Verdict::CompilerCandidate => "compiler-candidate",
    Verdict::Blocked => "blocked",
    Verdict::NeedsVerification => "needs-verification",
    Verdict::Unsupported => "unsupported",
    Verdict::NotApplicable => "not-applicable",
    Verdict::Ready => "ready",
  }
}

fn opt_in_of(file: &ProjectFile) -> OptIn {
  if file.facts.script.blocks.iter().any(|block| block.vapor) {
    OptIn::ScriptVaporAttr
  } else if file.facts.template.vapor {
    OptIn::TemplateVaporAttr
  } else {
    OptIn::None
  }
}

fn is_vapor_opted_in(file: &ProjectFile) -> bool {
  file.facts.template.vapor || file.facts.script.blocks.iter().any(|block| block.vapor)
}

fn assessment_span(file: &ProjectFile) -> SourceSpan {
  file
    .facts
    .script
    .blocks
    .iter()
    .find(|block| block.kind == ScriptKind::Setup)
    .and_then(|block| block.open_span)
    .or_else(|| file.facts.script.blocks.iter().find_map(|block| block.open_span))
    .or(file.facts.template.open_span)
    .unwrap_or_else(|| SourceSpan { offset: 0, length: file.source_len.min(1), line: 1, column: 1 })
}

fn is_vue_file(path: &Path) -> bool {
  path
    .extension()
    .and_then(|extension| extension.to_str())
    .is_some_and(|ext| ext.eq_ignore_ascii_case("vue"))
}

fn has_unaudited_dependency_closure(file: &ProjectFile) -> bool {
  file
    .facts
    .script
    .blocks
    .iter()
    .any(|block| block.imports.iter().any(|import| is_bare_non_vue(&import.source)))
}

fn is_bare_non_vue(source: &str) -> bool {
  if source.starts_with('.') || source.starts_with('/') || source.starts_with('#') {
    return false;
  }
  source != "vue" && !source.starts_with("vue/")
}

fn classify_toolchain(root: &Path) -> CheckOutcome {
  let mut resolved = Vec::new();
  let mut unknown = Vec::new();
  let mut compiler_sfc_35 = false;
  for (package, expected) in TOOLCHAIN_PACKAGES {
    match read_installed_version(root, package) {
      None => unknown.push(format!("toolchain: {package} unresolved")),
      Some(version) => {
        if *package == "@vue/compiler-sfc" && is_vue_35(&version) {
          compiler_sfc_35 = true;
        }
        resolved.push((*package, version, *expected));
      }
    }
  }
  if compiler_sfc_35 {
    return CheckOutcome {
      verdict: Verdict::Blocked,
      reasons: vec!["@vue/compiler-sfc 3.5.x ships no Vapor compiler".into()],
      unknown,
    };
  }
  if !unknown.is_empty() {
    return CheckOutcome { verdict: Verdict::NeedsVerification, reasons: unknown.clone(), unknown };
  }
  if resolved.iter().all(|(_, version, expected)| version == expected) {
    return CheckOutcome {
      verdict: Verdict::CompilerCandidate,
      reasons: vec![format!(
        "match vue {AUDITED_VUE_VERSION} / plugin-vue {AUDITED_PLUGIN_VUE_VERSION}"
      )],
      unknown,
    };
  }
  let versions = resolved
    .iter()
    .map(|(package, version, _)| format!("{package} {version}"))
    .collect::<Vec<_>>()
    .join(" / ");
  CheckOutcome {
    verdict: Verdict::Unsupported,
    reasons: vec![format!("resolved outside audited tuple: {versions}")],
    unknown,
  }
}

fn read_installed_version(root: &Path, package: &str) -> Option<String> {
  let path = root.join("node_modules").join(package).join("package.json");
  let text = fs::read_to_string(path).ok()?;
  let value: serde_json::Value = serde_json::from_str(&text).ok()?;
  value.get("version")?.as_str().map(str::to_owned)
}

fn is_vue_35(version: &str) -> bool {
  VueVersion::parse_requirement(version)
    .is_some_and(|parsed| parsed.major == 3 && parsed.minor == 5)
}

fn classify_ssr(files: &[&ProjectFile], project_context: &ProjectContext) -> CheckOutcome {
  if is_nuxt(project_context) || files.iter().any(|file| has_ssr_signal(file)) {
    return CheckOutcome {
      verdict: Verdict::NeedsVerification,
      reasons: vec!["SSR/hydration observed; hydrate unexecuted in the audited envelope".into()],
      unknown: Vec::new(),
    };
  }
  if files.iter().any(|file| has_create_app(file)) {
    return CheckOutcome {
      verdict: Verdict::NotApplicable,
      reasons: Vec::new(),
      unknown: Vec::new(),
    };
  }
  CheckOutcome {
    verdict: Verdict::NotApplicable,
    reasons: Vec::new(),
    unknown: vec!["app mode unresolved".into()],
  }
}

fn is_nuxt(project_context: &ProjectContext) -> bool {
  !project_context.nuxt_import_names.is_empty()
    || !project_context.nuxt_component_names.is_empty()
    || project_context.invalidation_inputs.iter().any(|input| {
      Path::new(input)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("nuxt.config"))
    })
}

fn has_ssr_signal(file: &ProjectFile) -> bool {
  file.facts.script.blocks.iter().any(|block| {
    block.imports.iter().any(|import| import.source == "vue/server-renderer")
      || block.calls.iter().any(|call| callee_is(&call.callee, "createSSRApp"))
  })
}

fn has_create_app(file: &ProjectFile) -> bool {
  file
    .facts
    .script
    .blocks
    .iter()
    .any(|block| block.calls.iter().any(|call| callee_is(&call.callee, "createApp")))
}

fn callee_is(callee: &str, name: &str) -> bool {
  callee == name || callee.rsplit('.').next() == Some(name)
}

struct SurfaceCheck {
  outcome: CheckOutcome,
  findings: Vec<LocatedReason>,
}

fn sfc_compile_contract(file: &ProjectFile) -> SurfaceCheck {
  let setup = file.facts.script.blocks.iter().any(|block| block.kind == ScriptKind::Setup);
  let script_vapor =
    file.facts.script.blocks.iter().any(|block| block.kind == ScriptKind::Script && block.vapor);
  let ordinary =
    file.facts.script.blocks.iter().find(|block| block.kind == ScriptKind::Script && !block.vapor);
  let mut findings = Vec::new();
  if script_vapor && let Some(span) = runtime_export_span(file) {
    findings.push(LocatedReason { reason: "setup block rejects runtime export".into(), span });
  }
  if ordinary.is_some() && setup {
    findings.push(LocatedReason {
      reason: "inherited Options API from ordinary script unverified".into(),
      span: ordinary.and_then(|block| block.open_span).unwrap_or_else(|| assessment_span(file)),
    });
  } else if ordinary.is_some() && !setup && !script_vapor && !file.facts.template.vapor {
    findings.push(LocatedReason {
      reason: "plugin force conversion ineligible: ordinary-script-only SFC".into(),
      span: ordinary.and_then(|block| block.open_span).unwrap_or_else(|| assessment_span(file)),
    });
  }
  if file.facts.template.vapor && ordinary.is_some() && !setup && !script_vapor {
    findings.push(LocatedReason {
      reason: "template-vapor with Options script: render + __vapor assembly unverified".into(),
      span: ordinary
        .and_then(|block| block.open_span)
        .or(file.facts.template.open_span)
        .unwrap_or_else(|| assessment_span(file)),
    });
  }
  let (verdict, reasons) = if findings.iter().any(|finding| {
    finding.reason.contains("ordinary-script-only")
      || finding.reason.contains("rejects runtime export")
  }) {
    (Verdict::Blocked, findings.iter().map(|finding| finding.reason.clone()).collect())
  } else if !findings.is_empty() {
    (Verdict::NeedsVerification, findings.iter().map(|finding| finding.reason.clone()).collect())
  } else {
    (Verdict::CompilerCandidate, Vec::new())
  };
  SurfaceCheck { outcome: CheckOutcome { verdict, reasons, unknown: Vec::new() }, findings }
}

fn runtime_export_span(file: &ProjectFile) -> Option<SourceSpan> {
  file
    .facts
    .script
    .blocks
    .iter()
    .find(|block| block.kind == ScriptKind::Script && block.vapor)
    .and_then(|block| block.runtime_export_spans.first().copied())
}

fn memo_contract(file: &ProjectFile) -> SurfaceCheck {
  let mut findings = Vec::new();
  for element in &file.facts.template.elements {
    for directive in &element.directives {
      if directive.name != "memo" {
        continue;
      }
      let expr = directive.expression.as_deref().unwrap_or("");
      findings.push(LocatedReason {
        reason: format!("v-memo `{expr}` is dropped under Vapor"),
        span: directive.span,
      });
    }
  }
  let verdict = if findings.is_empty() { Verdict::NotApplicable } else { Verdict::Blocked };
  let reasons = findings.iter().map(|finding| finding.reason.clone()).collect();
  SurfaceCheck { outcome: CheckOutcome { verdict, reasons, unknown: Vec::new() }, findings }
}

fn runtime_envelope(file: &ProjectFile) -> SurfaceCheck {
  let mut findings = Vec::new();
  for element in &file.facts.template.elements {
    let tag_key = comparable_name(&element.tag);
    if INTEROP_BUILTINS.contains(&tag_key.as_str()) {
      continue;
    }
    if tag_key == "slot" {
      findings.push(LocatedReason { reason: "slot".into(), span: element.span });
    }
    if is_dynamic_component(element) {
      findings.push(LocatedReason { reason: "dynamic component".into(), span: element.span });
    }
    for directive in &element.directives {
      if let Some(reason) = envelope_directive_reason(element, directive) {
        findings.push(LocatedReason { reason, span: directive.span });
      }
    }
  }
  for block in &file.facts.script.blocks {
    for call in &block.calls {
      if let Some(name) = envelope_script_api(call) {
        findings.push(LocatedReason { reason: name.into(), span: call.span });
      }
    }
  }
  findings.sort_by_key(|finding| finding.span);
  let verdict =
    if findings.is_empty() { Verdict::CompilerCandidate } else { Verdict::NeedsVerification };
  let reasons = findings.iter().map(|finding| finding.reason.clone()).collect();
  SurfaceCheck { outcome: CheckOutcome { verdict, reasons, unknown: Vec::new() }, findings }
}

fn envelope_directive_reason(
  element: &vue_vet_core::TemplateElementFact,
  directive: &vue_vet_core::TemplateDirectiveFact,
) -> Option<String> {
  if directive.name == "memo" {
    return None;
  }
  if ENVELOPE_UNTESTED_BUILT_IN_DIRECTIVES.contains(&directive.name.as_str()) {
    return Some(if directive.name == "slot" {
      "slot".into()
    } else {
      format!("v-{}", directive.name)
    });
  }
  if directive.name == "for" && !element.has_key() {
    return Some("unkeyed v-for".into());
  }
  if directive.name == "on" && !directive.modifiers.is_empty() {
    let event = directive.argument.as_deref().unwrap_or("event");
    let modifiers = directive.modifiers.join(".");
    return Some(format!("@{event}.{modifiers}"));
  }
  if ENVELOPE_VUE_BUILT_IN_DIRECTIVES.contains(&directive.name.as_str()) {
    return None;
  }
  Some(format!("custom directive v-{}", directive.name))
}

fn is_dynamic_component(element: &vue_vet_core::TemplateElementFact) -> bool {
  element.tag.eq_ignore_ascii_case("component")
    || element.directive("is").is_some()
    || element.bound_attribute("is").is_some()
}

fn envelope_script_api(call: &ScriptCallFact) -> Option<&'static str> {
  ENVELOPE_SCRIPT_APIS.iter().copied().find(|name| {
    callee_is(&call.callee, name)
      || call.resolved_import.as_ref().is_some_and(|(_, exported)| exported == name)
  })
}

fn interop_required(file: &ProjectFile, children: &ChildIndex<'_>) -> SurfaceCheck {
  let from = format!("file:{}", normalized_path(file.path.as_path()));
  let mut findings = Vec::new();
  let mut unknown = Vec::new();
  for element in &file.facts.template.elements {
    let key = comparable_name(&element.tag);
    if INTEROP_BUILTINS.contains(&key.as_str()) {
      findings.push(LocatedReason {
        reason: "requires vaporInteropPlugin; not executed in the audited envelope".into(),
        span: element.span,
      });
      continue;
    }
    if !element.is_component {
      continue;
    }
    match children.resolve(&from, &element.tag) {
      ChildResolve::Unresolved => {
        unknown.push(format!("child component {} unresolved", element.tag));
      }
      ChildResolve::NonVapor => {
        findings.push(LocatedReason {
          reason: "requires vaporInteropPlugin; not executed in the audited envelope".into(),
          span: element.span,
        });
      }
      ChildResolve::Vapor => {}
    }
  }
  let verdict =
    if findings.is_empty() { Verdict::NotApplicable } else { Verdict::NeedsVerification };
  let reasons = findings.iter().map(|finding| finding.reason.clone()).collect();
  SurfaceCheck { outcome: CheckOutcome { verdict, reasons, unknown }, findings }
}

enum ChildResolve {
  Vapor,
  NonVapor,
  Unresolved,
}

fn comparable_name(name: &str) -> String {
  name.chars().filter(char::is_ascii_alphanumeric).flat_map(char::to_lowercase).collect()
}

fn migration_diagnostic(
  rule_id: &str,
  documentation: &str,
  file: &ProjectFile,
  span: SourceSpan,
  message: String,
  assessment: Option<Assessment>,
) -> Diagnostic {
  Diagnostic {
    rule_id: rule_id.into(),
    category: MIGRATION_CATEGORY.into(),
    severity: vue_vet_core::Severity::Info,
    confidence: Some(Confidence::High),
    documentation: Some(documentation.into()),
    message,
    help: None,
    file: file.path.clone(),
    span,
    edits: Vec::new(),
    recommendation: None,
    assessment,
  }
}

#[cfg(test)]
#[expect(
  clippy::expect_used,
  clippy::let_underscore_must_use,
  reason = "unit tests fail closed on fixture I/O"
)]
mod tests {
  use std::path::PathBuf;

  use super::*;

  #[test]
  fn aggregate_orders_blocked_over_unsupported_and_ignores_not_applicable() {
    let checks = vec![
      AssessmentCheck { check: "a".into(), verdict: Verdict::NotApplicable, reasons: Vec::new() },
      AssessmentCheck {
        check: "b".into(),
        verdict: Verdict::CompilerCandidate,
        reasons: Vec::new(),
      },
      AssessmentCheck { check: "c".into(), verdict: Verdict::Unsupported, reasons: Vec::new() },
      AssessmentCheck { check: "d".into(), verdict: Verdict::Blocked, reasons: Vec::new() },
    ];
    assert_eq!(Assessment::aggregate_verdict(&checks, true), Verdict::Blocked);
  }

  #[test]
  fn incomplete_cannot_beat_needs_verification() {
    let checks = vec![AssessmentCheck {
      check: "toolchain-tuple".into(),
      verdict: Verdict::CompilerCandidate,
      reasons: Vec::new(),
    }];
    assert_eq!(Assessment::aggregate_verdict(&checks, false), Verdict::NeedsVerification);
  }

  #[test]
  fn complete_envelope_promotes_to_ready() {
    let checks = vec![
      AssessmentCheck {
        check: "toolchain-tuple".into(),
        verdict: Verdict::CompilerCandidate,
        reasons: Vec::new(),
      },
      AssessmentCheck {
        check: "runtime-envelope".into(),
        verdict: Verdict::CompilerCandidate,
        reasons: Vec::new(),
      },
    ];
    assert_eq!(Assessment::aggregate_verdict(&checks, true), Verdict::Ready);
    assert_eq!(Assessment::convertible_of(&checks, true), Convertible::Yes);
    assert_eq!(Assessment::aggregate_verdict(&checks, false), Verdict::NeedsVerification);
    assert_eq!(Assessment::convertible_of(&checks, false), Convertible::Unknown);
  }

  #[test]
  fn toolchain_classifies_matched_unresolved_and_compiler_sfc_35() {
    let matched = TempTree::with_versions(
      "matched",
      &[
        ("vue", AUDITED_VUE_VERSION),
        ("@vue/compiler-sfc", AUDITED_VUE_VERSION),
        ("@vue/compiler-vapor", AUDITED_VUE_VERSION),
        ("@vue/runtime-vapor", AUDITED_VUE_VERSION),
        ("@vitejs/plugin-vue", AUDITED_PLUGIN_VUE_VERSION),
      ],
    );
    let outcome = classify_toolchain(&matched.root);
    assert_eq!(outcome.verdict, Verdict::CompilerCandidate);
    assert!(outcome.unknown.is_empty());

    let gap = TempTree::with_versions(
      "gap",
      &[
        ("vue", AUDITED_VUE_VERSION),
        ("@vue/compiler-sfc", "3.5.42"),
        ("@vue/compiler-vapor", AUDITED_VUE_VERSION),
        ("@vue/runtime-vapor", AUDITED_VUE_VERSION),
        ("@vitejs/plugin-vue", AUDITED_PLUGIN_VUE_VERSION),
      ],
    );
    let blocked = classify_toolchain(&gap.root);
    assert_eq!(blocked.verdict, Verdict::Blocked);
    assert!(blocked.reasons.iter().any(|reason| reason.contains("3.5.x")));

    let empty = TempTree::with_versions("empty", &[]);
    let unresolved = classify_toolchain(&empty.root);
    assert_eq!(unresolved.verdict, Verdict::NeedsVerification);
    assert!(unresolved.unknown.iter().any(|reason| reason.contains("vue unresolved")));
  }

  struct TempTree {
    root: PathBuf,
  }

  impl TempTree {
    fn with_versions(label: &str, packages: &[(&str, &str)]) -> Self {
      let root =
        std::env::temp_dir().join(format!("vv-vapor-toolchain-{}-{}", std::process::id(), label));
      let _ = fs::remove_dir_all(&root);
      fs::create_dir_all(&root).expect("temp root");
      for (package, version) in packages {
        let dir = root.join("node_modules").join(package);
        fs::create_dir_all(&dir).expect("pkg dir");
        fs::write(
          dir.join("package.json"),
          format!("{{\"name\":\"{package}\",\"version\":\"{version}\"}}"),
        )
        .expect("pkg json");
      }
      Self { root }
    }
  }

  impl Drop for TempTree {
    fn drop(&mut self) {
      let _ = fs::remove_dir_all(&self.root);
    }
  }
}
