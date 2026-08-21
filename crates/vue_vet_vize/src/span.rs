//! SFC byte-offset → line/column using the analysis-scoped line index.
use std::cell::RefCell;
use std::sync::Arc;

use vue_vet_core::SourceSpan;

thread_local! {
  /// Installed for one SFC analysis so span mapping can reuse the LineIndex.
  static SFC_LINE_INDEX: RefCell<Option<Arc<vue_vet_core::LineIndex>>> = const { RefCell::new(None) };
}

pub fn install_line_index(index: Arc<vue_vet_core::LineIndex>) {
  SFC_LINE_INDEX.with(|slot| {
    *slot.borrow_mut() = Some(index);
  });
}

pub fn clear_line_index() {
  SFC_LINE_INDEX.with(|slot| {
    *slot.borrow_mut() = None;
  });
}

pub fn position_offset(offset: u32) -> usize {
  usize::try_from(offset).unwrap_or(usize::MAX)
}

pub fn source_span(source: &str, offset: usize, length: usize) -> SourceSpan {
  let (line, column) = line_column(source, offset);
  SourceSpan { offset, length, line, column }
}

fn line_column(source: &str, offset: usize) -> (usize, usize) {
  SFC_LINE_INDEX.with(|slot| {
    slot.borrow().as_ref().map_or_else(
      || vue_vet_core::LineIndex::new(source).byte_to_line_column(offset),
      |index| index.as_ref().byte_to_line_column(offset),
    )
  })
}
