//! Shared source text + line index for analysis and editor surfaces.

use std::sync::Arc;

use crate::line_index::LineIndex;

/// One source buffer with a precomputed [`LineIndex`].
///
/// Build once at parse / open-document boundaries and share via `Arc` so
/// diagnostics, tracers, and LSP conversions do not rebuild line starts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceContext {
  text: Arc<str>,
  line_index: Arc<LineIndex>,
}

impl SourceContext {
  /// Index `text` once and retain both handles.
  #[must_use]
  pub fn new(text: impl Into<Arc<str>>) -> Self {
    let text = text.into();
    let line_index = Arc::new(LineIndex::new(text.as_ref()));
    Self { text, line_index }
  }

  /// Borrow the source text.
  #[must_use]
  pub fn text(&self) -> &str {
    self.text.as_ref()
  }

  /// Shared text handle.
  #[must_use]
  pub fn text_arc(&self) -> Arc<str> {
    Arc::clone(&self.text)
  }

  /// Shared line index.
  #[must_use]
  pub fn line_index(&self) -> &LineIndex {
    self.line_index.as_ref()
  }

  /// Shared line-index handle.
  #[must_use]
  pub fn line_index_arc(&self) -> Arc<LineIndex> {
    Arc::clone(&self.line_index)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn source_context_indexes_once_and_shares_arcs() {
    let ctx = SourceContext::new("a\nb\n");
    assert_eq!(ctx.text(), "a\nb\n");
    assert_eq!(ctx.line_index().byte_to_line_column(2), (2, 1));
    assert!(Arc::ptr_eq(&ctx.text_arc(), &ctx.text_arc()));
    assert!(Arc::ptr_eq(&ctx.line_index_arc(), &ctx.line_index_arc()));
  }
}
