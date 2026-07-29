//! ANSI styling for interactive text reports.
//!
//! Machine formats and snapshots keep `enabled = false`. The CLI decides when
//! to enable color (`--color`, TTY, `NO_COLOR` / `FORCE_COLOR`).

use anstyle::{AnsiColor, Color, Style};
use vue_vet_core::Severity;

const LOCATION: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Cyan)));
const RULE: Style = Style::new().dimmed();
const HELP: Style = Style::new().dimmed();
const HEADER: Style = Style::new().bold();
const SCORE_LABEL: Style = Style::new().bold();
const SCORE_VALUE: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Cyan)));

const ERROR: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Red))).bold();
const WARNING: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Yellow))).bold();
const INFO: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Cyan)));

#[must_use]
pub fn paint(style: Style, text: &str, enabled: bool) -> String {
  if !enabled || text.is_empty() {
    return text.to_owned();
  }
  format!("{style}{text}{}", style.render_reset())
}

#[must_use]
pub const fn severity_style(severity: Severity) -> Style {
  match severity {
    Severity::Error => ERROR,
    Severity::Warning => WARNING,
    Severity::Info => INFO,
  }
}

#[must_use]
pub fn location(text: &str, enabled: bool) -> String {
  paint(LOCATION, text, enabled)
}

#[must_use]
pub fn rule_id(text: &str, enabled: bool) -> String {
  paint(RULE, text, enabled)
}

#[must_use]
pub fn help_prefix(enabled: bool) -> String {
  paint(HELP, "help:", enabled)
}

#[must_use]
pub fn recommend_prefix(enabled: bool) -> String {
  paint(HELP, "recommend:", enabled)
}

#[must_use]
pub fn header(text: &str, enabled: bool) -> String {
  paint(HEADER, text, enabled)
}

#[must_use]
pub fn score_label(enabled: bool) -> String {
  paint(SCORE_LABEL, "Vue Vet score", enabled)
}

#[must_use]
pub fn score_value(text: &str, enabled: bool) -> String {
  paint(SCORE_VALUE, text, enabled)
}

#[must_use]
pub fn severity_label(severity: Severity, label: &str, enabled: bool) -> String {
  paint(severity_style(severity), label, enabled)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn paint_disabled_is_identity() {
    assert_eq!(paint(ERROR, "error", false), "error");
  }

  #[test]
  fn paint_enabled_wraps_ansi() {
    let painted = paint(ERROR, "error", true);
    assert!(painted.contains('\u{1b}'), "expected ANSI: {painted:?}");
    assert!(painted.contains("error"), "expected payload: {painted:?}");
  }
}
