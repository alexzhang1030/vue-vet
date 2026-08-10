//! Static tracking-scope explanation — “would Vue re-run this?”
//!
//! Pure consumers of [`ReactivityGraph`] / [`TrackingScopeFact`]. No Oxc, no I/O.

use vue_vet_core::{
  ReactiveReadKind, ReactivityGraph, ScopeExplain, ScopeExplainDep, ScopeTrackReason,
  TrackingScopeFact, TrackingScopeKind,
};

/// Build a multi-consumer explain payload for one tracking scope.
#[must_use]
pub fn explain_tracking_scope(module_id: &str, scope: &TrackingScopeFact) -> ScopeExplain {
  let mut tracks = Vec::new();
  let mut does_not_track = Vec::new();
  for read in &scope.reads {
    let dep = dep_from_read(read);
    match read.kind {
      ReactiveReadKind::Unconditional | ReactiveReadKind::Conditional => tracks.push(dep),
      ReactiveReadKind::AfterAwait | ReactiveReadKind::OutsideTracking => {
        does_not_track.push(dep);
      }
    }
  }
  tracks.sort_by(|left, right| {
    (left.path.as_str(), left.reason, left.span.offset).cmp(&(
      right.path.as_str(),
      right.reason,
      right.span.offset,
    ))
  });
  does_not_track.sort_by(|left, right| {
    (left.path.as_str(), left.reason, left.span.offset).cmp(&(
      right.path.as_str(),
      right.reason,
      right.span.offset,
    ))
  });

  let mut uncertain = scope.uncertain_accesses.clone();
  uncertain.sort();
  uncertain.dedup();

  let summary = scope_summary(scope, &tracks, &does_not_track, &uncertain);
  ScopeExplain {
    module_id: module_id.into(),
    kind: scope_kind_label(scope.kind).into(),
    callee: scope.callee.clone(),
    binding: scope.binding.clone(),
    span: scope.span.clone(),
    summary,
    tracks,
    does_not_track,
    uncertain,
  }
}

/// Select scopes in a graph matching a human query.
///
/// Supported forms (case-sensitive for names):
/// - `binding` — `scope.binding == query`
/// - `module:binding` — module id ends with / equals left, binding matches right
/// - `module@offset` or `@offset` — span start equals offset
/// - `callee@offset` — callee + span
#[must_use]
pub fn select_tracking_scopes<'graph>(
  module_id: &str,
  graph: &'graph ReactivityGraph,
  query: &str,
) -> Vec<&'graph TrackingScopeFact> {
  let query = query.trim();
  if query.is_empty() {
    return Vec::new();
  }

  let mut selected = Vec::new();
  for scope in &graph.scopes {
    if scope_matches(module_id, scope, query) {
      selected.push(scope);
    }
  }
  selected
}

/// Find the tightest tracking scope covering a diagnostic span (same module).
#[must_use]
pub fn scope_covering_span(
  graph: &ReactivityGraph,
  offset: usize,
  length: usize,
) -> Option<&TrackingScopeFact> {
  let end = offset.saturating_add(length);
  graph
    .scopes
    .iter()
    .filter(|scope| {
      let scope_end = scope.span.offset.saturating_add(scope.span.length);
      scope.span.offset <= offset && end <= scope_end
    })
    .min_by_key(|scope| (scope.span.length, scope.span.offset))
}

fn scope_matches(module_id: &str, scope: &TrackingScopeFact, query: &str) -> bool {
  if let Some((left, right)) = query.split_once(':') {
    let module_ok = module_id == left
      || module_id.ends_with(left)
      || module_id.ends_with(&format!("/{left}"))
      || PathTail(module_id).ends_with(left);
    return module_ok && scope_matches(module_id, scope, right);
  }
  if let Some(offset_text) = query.strip_prefix('@') {
    return offset_text.parse::<usize>().ok() == Some(scope.span.offset);
  }
  if let Some((left, right)) = query.split_once('@')
    && let Ok(offset) = right.parse::<usize>()
  {
    let left_ok = scope.callee == left
      || scope.binding.as_deref() == Some(left)
      || module_id == left
      || module_id.ends_with(left)
      || module_id.ends_with(&format!("/{left}"));
    return left_ok && scope.span.offset == offset;
  }
  scope.binding.as_deref() == Some(query)
    || scope.callee == query
    || format!("{}({})", scope_kind_label(scope.kind), scope.callee) == query
}

/// Tiny helper so we do not pull `Path` into every call for suffix checks.
struct PathTail<'a>(&'a str);
impl PathTail<'_> {
  fn ends_with(&self, suffix: &str) -> bool {
    self.0 == suffix
      || self.0.ends_with(&format!("/{suffix}"))
      || self.0.ends_with(&format!("\\{suffix}"))
  }
}

fn dep_from_read(read: &vue_vet_core::ReactiveReadFact) -> ScopeExplainDep {
  let path = match read.property.as_deref() {
    Some(property) if !property.is_empty() => format!("{}.{}", read.binding, property),
    _ => read.binding.clone(),
  };
  let (reason, reason_label) = match read.kind {
    ReactiveReadKind::Unconditional => {
      (ScopeTrackReason::Unconditional, "always tracked while this scope runs")
    }
    ReactiveReadKind::Conditional => {
      (ScopeTrackReason::Conditional, "tracked only on some control-flow paths")
    }
    ReactiveReadKind::AfterAwait => (ScopeTrackReason::AfterAwait, "not tracked (after await)"),
    ReactiveReadKind::OutsideTracking => (
      ScopeTrackReason::OutsideTracking,
      "not tracked (outside active tracking: then/nextTick/callback)",
    ),
  };
  let mut guards = read
    .guards
    .iter()
    .map(|guard| {
      guard
        .property
        .as_deref()
        .map_or_else(|| guard.binding.clone(), |property| format!("{}.{}", guard.binding, property))
    })
    .collect::<Vec<_>>();
  guards.sort();
  guards.dedup();
  ScopeExplainDep {
    binding: read.binding.clone(),
    property: read.property.clone(),
    path,
    reason,
    reason_label: reason_label.into(),
    span: read.span.clone(),
    guards,
  }
}

fn scope_summary(
  scope: &TrackingScopeFact,
  tracks: &[ScopeExplainDep],
  does_not_track: &[ScopeExplainDep],
  uncertain: &[String],
) -> String {
  let who = scope.binding.as_deref().map_or_else(|| scope.callee.as_str(), |name| name);
  if tracks.is_empty() && uncertain.is_empty() {
    if does_not_track.is_empty() {
      return format!(
        "`{who}` has no known reactive dependency — Vue will not re-run it when state changes"
      );
    }
    return format!(
      "`{who}` has no known tracked dependency ({} read(s) outside tracking)",
      does_not_track.len()
    );
  }
  if tracks.is_empty() {
    return format!(
      "`{who}` has no proven dependency; soft evidence maybe:{}",
      uncertain.join(",")
    );
  }
  let unconditional =
    tracks.iter().filter(|dep| dep.reason == ScopeTrackReason::Unconditional).count();
  let conditional = tracks.len().saturating_sub(unconditional);
  let mut parts = vec![format!("`{who}` tracks {} dependency path(s)", tracks.len())];
  if unconditional > 0 {
    parts.push(format!("{unconditional} unconditional"));
  }
  if conditional > 0 {
    parts.push(format!("{conditional} conditional"));
  }
  if !does_not_track.is_empty() {
    parts.push(format!("{} not tracked", does_not_track.len()));
  }
  if !uncertain.is_empty() {
    parts.push(format!("maybe:{}", uncertain.join(",")));
  }
  // "a; b; c" style
  let head = parts.remove(0);
  if parts.is_empty() { head } else { format!("{head} ({})", parts.join(", ")) }
}

const fn scope_kind_label(kind: TrackingScopeKind) -> &'static str {
  match kind {
    TrackingScopeKind::WatchEffect => "watch_effect",
    TrackingScopeKind::WatchPostEffect => "watch_post_effect",
    TrackingScopeKind::WatchSyncEffect => "watch_sync_effect",
    TrackingScopeKind::Computed => "computed",
    TrackingScopeKind::WatchSources => "watch_sources",
    TrackingScopeKind::WatchCallback => "watch_callback",
    TrackingScopeKind::EffectScope => "effect_scope",
    TrackingScopeKind::OnScopeDispose => "on_scope_dispose",
    TrackingScopeKind::Render => "render",
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use vue_vet_core::{ReactiveReadFact, ReactiveReadKind, SourceSpan, TrackingScopeKind};

  fn span(offset: usize) -> SourceSpan {
    SourceSpan { offset, length: 4, line: 1, column: 1 }
  }

  fn read(binding: &str, kind: ReactiveReadKind, offset: usize) -> ReactiveReadFact {
    ReactiveReadFact {
      binding: binding.into(),
      property: Some("value".into()),
      kind,
      guards: Vec::new(),
      guarded_by: None,
      span: span(offset),
    }
  }

  #[test]
  fn empty_scope_summarizes_no_dependency() {
    let scope = TrackingScopeFact {
      kind: TrackingScopeKind::Computed,
      callee: "computed".into(),
      span: span(10),
      reads: Vec::new(),
      writes: Vec::new(),
      assignment_only: false,
      binding: Some("label".into()),
      uncertain_accesses: Vec::new(),
    };
    let explain = explain_tracking_scope("App.vue", &scope);
    assert!(explain.tracks.is_empty());
    assert!(explain.summary.contains("no known reactive dependency"));
  }

  #[test]
  fn splits_tracked_and_outside() {
    let scope = TrackingScopeFact {
      kind: TrackingScopeKind::WatchEffect,
      callee: "watchEffect".into(),
      span: span(0),
      reads: vec![
        read("count", ReactiveReadKind::Unconditional, 1),
        read("other", ReactiveReadKind::OutsideTracking, 2),
      ],
      writes: Vec::new(),
      assignment_only: false,
      binding: None,
      uncertain_accesses: vec!["maybeRoot".into()],
    };
    let explain = explain_tracking_scope("m.ts", &scope);
    assert_eq!(explain.tracks.len(), 1);
    assert_eq!(explain.tracks.first().map(|dep| dep.path.as_str()), Some("count.value"));
    assert_eq!(explain.does_not_track.len(), 1);
    assert!(explain.summary.contains("tracks 1"));
    assert_eq!(explain.uncertain, vec!["maybeRoot".to_string()]);
  }

  #[test]
  fn select_by_binding_and_offset() {
    let scope = TrackingScopeFact {
      kind: TrackingScopeKind::Computed,
      callee: "computed".into(),
      span: span(42),
      reads: Vec::new(),
      writes: Vec::new(),
      assignment_only: false,
      binding: Some("doubled".into()),
      uncertain_accesses: Vec::new(),
    };
    let graph = ReactivityGraph { scopes: vec![scope], ..ReactivityGraph::default() };
    assert_eq!(select_tracking_scopes("App.vue", &graph, "doubled").len(), 1);
    assert_eq!(select_tracking_scopes("App.vue", &graph, "App.vue:doubled").len(), 1);
    assert_eq!(select_tracking_scopes("App.vue", &graph, "@42").len(), 1);
    assert_eq!(select_tracking_scopes("App.vue", &graph, "computed@42").len(), 1);
    assert!(select_tracking_scopes("App.vue", &graph, "missing").is_empty());
  }
}
