//! Watch-option maps and the shared closed-options read.

use std::collections::HashMap;

use oxc_span::Span;

use super::{Indexes, Literal, OptionValue, WatchConsumerOptions};

#[derive(Default)]
pub(in crate::source_contracts) struct OptionsIndex {
  pub watch_options: HashMap<u64, WatchConsumerOptions>,
}

/// Keys a caller already proves. Unrequested keys stay `Absent` and are not read.
pub(in crate::source_contracts) struct ClosedOptions {
  pub flush: OptionValue<Literal>,
  pub immediate: OptionValue<Literal>,
  pub once: OptionValue<Literal>,
  pub deep: OptionValue<Literal>,
}

impl Indexes {
  /// One options read for the keys this caller uses. Each requested key is
  /// `option_value`: missing or `undefined` is absent, a non-literal is unknown.
  #[expect(
    clippy::fn_params_excessive_bools,
    reason = "each flag is one watch option the caller already proves"
  )]
  pub(in crate::source_contracts) fn closed_options(
    &self,
    object: Span,
    flush: bool,
    immediate: bool,
    once: bool,
    deep: bool,
  ) -> ClosedOptions {
    ClosedOptions {
      flush: self.closed_flag(object, flush, "flush"),
      immediate: self.closed_flag(object, immediate, "immediate"),
      once: self.closed_flag(object, once, "once"),
      deep: self.closed_flag(object, deep, "deep"),
    }
  }

  fn closed_flag(&self, object: Span, read: bool, key: &str) -> OptionValue<Literal> {
    if read { self.option_value(object, key) } else { OptionValue::Absent }
  }
}
