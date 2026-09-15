//! Template allocation relations and joined template-ref demand facts.
//!
//! Produced while the Vize tree is available and joined with Oxc
//! source/owner/value/flush/demand facts. No parser AST types.

use serde::{Deserialize, Serialize};

use crate::diagnostics::SourceSpan;

/// Parent / memo / condition / ref relations for one template element.
///
/// Element spans cover the start tag only, so descendant ownership is recorded
/// during the Vize walk rather than recovered by span containment.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[expect(
  clippy::struct_excessive_bools,
  reason = "independent allocation flags recorded while the Vize tree is available"
)]
pub struct TemplateAllocationFact {
  pub element_span: SourceSpan,
  pub tag: String,
  /// Vize component node or later import-marked component. Native HTML/SVG
  /// tags stay false even when a same-name binding is imported.
  #[serde(default, skip_serializing_if = "is_false")]
  pub is_component: bool,
  /// Static `ref="name"` (not `:ref` / callback).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub static_ref: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub ref_span: Option<SourceSpan>,
  /// Immediate parent element start-tag span recorded in the walk.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub parent_span: Option<SourceSpan>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub condition: Option<TemplateConditionRelation>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub memo: Option<TemplateMemoRelation>,
  #[serde(default, skip_serializing_if = "is_false")]
  pub v_show: bool,
  #[serde(default, skip_serializing_if = "is_false")]
  pub v_for: bool,
  #[serde(default, skip_serializing_if = "is_false")]
  pub slot: bool,
  #[serde(default, skip_serializing_if = "is_false")]
  pub transition: bool,
  /// A memo ancestor exists in addition to `memo` on this node.
  #[serde(default, skip_serializing_if = "is_false")]
  pub nested_memo: bool,
  /// Bound / callback `ref` (`:ref` / `v-bind:ref`).
  #[serde(default, skip_serializing_if = "is_false")]
  pub callback_ref: bool,
  /// `v-if` / `v-else-if` sits strictly inside the memo subtree, so Vue can
  /// skip creating this node when memo deps stay still. Same-element or
  /// ancestor `v-if` (evaluated outside `withMemo`) stays false.
  #[serde(default, skip_serializing_if = "is_false")]
  pub condition_inside_memo: bool,
}

/// Innermost `v-if` / `v-else-if` that allocates this node.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TemplateConditionRelation {
  pub expression: String,
  pub span: SourceSpan,
  #[serde(default)]
  pub identifiers: Option<Vec<String>>,
  /// Proven single identifier (`visible`), not a compound expression.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub simple_identifier: Option<String>,
  /// True when the condition directive is on this element, not only an ancestor.
  #[serde(default, skip_serializing_if = "is_false")]
  pub on_self: bool,
}

/// Innermost `v-memo` that caches this node's subtree.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TemplateMemoRelation {
  pub expression: String,
  pub span: SourceSpan,
  #[serde(default)]
  pub identifiers: Option<Vec<String>>,
  /// `v-memo="[]"`.
  #[serde(default, skip_serializing_if = "is_false")]
  pub empty: bool,
  /// Every array element is a literal or identifier (no calls, spreads, members).
  #[serde(default, skip_serializing_if = "is_false")]
  pub stable_tuple: bool,
  #[serde(default, skip_serializing_if = "is_false")]
  pub on_self: bool,
}

/// Proven pre-flush demand of a still-null conditional template ref.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PreFlushTemplateRefDemandFact {
  pub demand_span: SourceSpan,
  pub condition_write_span: SourceSpan,
  pub watch_span: SourceSpan,
  pub allocation_span: SourceSpan,
  /// Effective `'pre'` or `'sync'`.
  pub flush: String,
}

/// Proven after-tick demand of a memo-blocked conditional template ref.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[expect(clippy::struct_field_names, reason = "each field is a distinct causal span")]
pub struct MemoBlockedRefDemandFact {
  pub demand_span: SourceSpan,
  pub memo_span: SourceSpan,
  pub condition_span: SourceSpan,
  pub ref_span: SourceSpan,
  pub source_change_span: SourceSpan,
  pub render_boundary_span: SourceSpan,
}

/// Joined template-ref demand facts for one script block.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TemplateRefDemandFacts {
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub pre_flush: Vec<PreFlushTemplateRefDemandFact>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub memo_blocked: Vec<MemoBlockedRefDemandFact>,
}

impl TemplateRefDemandFacts {
  #[must_use]
  pub const fn is_empty(&self) -> bool {
    self.pre_flush.is_empty() && self.memo_blocked.is_empty()
  }

  pub fn sort_by_source_order(&mut self) {
    self.pre_flush.sort_by_key(|fact| fact.demand_span.offset);
    self.memo_blocked.sort_by_key(|fact| fact.demand_span.offset);
  }
}

#[expect(clippy::trivially_copy_pass_by_ref, reason = "serde skip_serializing_if takes &T")]
const fn is_false(value: &bool) -> bool {
  !*value
}
