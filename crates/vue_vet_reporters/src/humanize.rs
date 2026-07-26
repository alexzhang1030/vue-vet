//! Turn tracer machine labels into short Vue-facing phrases.
//!
//! Shared by the text/JSON digest and the reactivity TUI so editor and terminal
//! surfaces stay wording-consistent.

fn split_edge(edge: &str) -> Option<(&str, &str)> {
  edge.split_once(" -> ")
}

fn split_template_read(label: &str) -> Option<(&str, &str)> {
  label.split_once('@')
}

fn strip_offset(source: &str) -> &str {
  source.split_once('@').map_or(source, |(head, _)| head)
}

/// Turn a scope/template/`from` machine label into a short Vue-facing phrase.
#[must_use]
pub fn humanize_source(source: &str) -> String {
  let source = strip_offset(source);
  if let Some(surface) = source.strip_prefix("template:") {
    return humanize_template_surface(surface);
  }
  if let Some((kind, rest)) = source.split_once(':') {
    return match kind {
      "watch_sources" => format!("watch({})", rest.trim_start_matches("watch")),
      "watch_callback" => "watch callback".into(),
      "watch_effect" => "watchEffect()".into(),
      "watch_post_effect" => "watchPostEffect()".into(),
      "watch_sync_effect" => "watchSyncEffect()".into(),
      "computed" => {
        if rest.is_empty() {
          "computed()".into()
        } else {
          format!("computed({rest})")
        }
      }
      "effect_scope" => "effectScope()".into(),
      "on_scope_dispose" => "onScopeDispose()".into(),
      _ => format!("{kind}({rest})"),
    };
  }
  match source {
    "watchEffect" => "watchEffect()".into(),
    "watchPostEffect" => "watchPostEffect()".into(),
    "watchSyncEffect" => "watchSyncEffect()".into(),
    "computed" => "computed()".into(),
    "watch" => "watch()".into(),
    other => other.into(),
  }
}

#[must_use]
pub fn humanize_template_surface(surface: &str) -> String {
  match surface {
    "if" => "v-if".into(),
    "else-if" => "v-else-if".into(),
    "show" => "v-show".into(),
    "for" => "v-for".into(),
    "on" => "v-on / @".into(),
    "bind" => "v-bind / :".into(),
    "model" => "v-model".into(),
    "slot" => "v-slot".into(),
    "text" | "interpolation" => "{{ }}".into(),
    "html" => "v-html".into(),
    "class" => ":class".into(),
    "style" => ":style".into(),
    "ref" => "ref=".into(),
    other => format!("template:{other}"),
  }
}

#[must_use]
pub fn humanize_edge(edge: &str) -> String {
  split_edge(edge)
    .map_or_else(|| edge.to_owned(), |(from, to)| format!("{}  →  {to}", humanize_source(from)))
}

#[must_use]
pub fn humanize_edge_parts(from: &str, to: &str) -> String {
  format!("{}  →  {to}", humanize_source(from))
}

#[must_use]
pub fn humanize_binding(binding: &str) -> String {
  binding.split_once(':').map_or_else(
    || binding.to_owned(),
    |(name, kind)| format!("{name}  ({})", kind.replace('_', " ")),
  )
}

#[must_use]
pub fn humanize_binding_parts(name: &str, kind: &str) -> String {
  format!("{name}  ({})", kind.replace('_', " "))
}

#[must_use]
pub fn humanize_scope(scope: &str) -> String {
  if let Some((kind, rest)) = scope.split_once('(') {
    let inner = rest.trim_end_matches(')');
    let label = match kind {
      "watch_effect" => "watchEffect",
      "watch_post_effect" => "watchPostEffect",
      "watch_sync_effect" => "watchSyncEffect",
      "watch_sources" => "watch",
      "watch_callback" => "watch callback",
      "computed" => "computed",
      "effect_scope" => "effectScope",
      "on_scope_dispose" => "onScopeDispose",
      other => other,
    };
    if inner.is_empty() { format!("{label}()") } else { format!("{label}({inner})") }
  } else {
    humanize_source(scope)
  }
}

#[must_use]
pub fn humanize_template_read(label: &str) -> String {
  split_template_read(label).map_or_else(
    || label.to_owned(),
    |(binding, surface)| format!("{}  reads  {binding}", humanize_template_surface(surface)),
  )
}

#[must_use]
pub fn humanize_template_read_parts(binding: &str, surface: &str) -> String {
  format!("{}  reads  {binding}", humanize_template_surface(surface))
}

/// Parse `{name}@{offset}` identities used by graph v6 `to_id` / template labels.
#[must_use]
pub fn parse_name_offset(identity: &str) -> Option<(&str, usize)> {
  let (name, offset) = identity.rsplit_once('@')?;
  let offset = offset.parse().ok()?;
  Some((name, offset))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn humanizes_template_and_watch_edges() {
    assert_eq!(humanize_edge("template:if@11768 -> error"), "v-if  →  error");
    assert_eq!(humanize_edge("template:interpolation@12154 -> hint"), "{{ }}  →  hint");
    assert_eq!(humanize_edge("template:class@14082 -> backend"), ":class  →  backend");
    assert_eq!(humanize_edge("watch_sources:watch@11110 -> backend"), "watch()  →  backend");
  }

  #[test]
  fn parses_span_qualified_identities() {
    assert_eq!(parse_name_offset("error@420"), Some(("error", 420)));
    assert_eq!(parse_name_offset("no-at"), None);
  }
}
