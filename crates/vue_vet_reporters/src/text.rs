//! Human text report lines and score footer.
use vue_vet_core::{Diagnostic, PRACTICE_CATEGORY, ScanSummary, Severity};

use crate::{ReportContext, color, render_reactivity_footer};

pub fn render_text(summary: &ScanSummary, context: &ReportContext) -> String {
  let mut output = render_text_diagnostics(&summary.diagnostics, context.color);
  output.push('\n');
  output.push_str(&render_text_score_footer(summary, context));
  output
}

/// Render lint + practice diagnostics as text lines (no score footer).
#[must_use]
pub fn render_text_diagnostics(diagnostics: &[Diagnostic], color: bool) -> String {
  let mut output = String::new();
  let (lint, practice): (Vec<_>, Vec<_>) =
    diagnostics.iter().partition(|diagnostic| diagnostic.category != PRACTICE_CATEGORY);
  for diagnostic in &lint {
    append_text_diagnostic(&mut output, diagnostic, color);
  }
  if !practice.is_empty() {
    if !lint.is_empty() {
      output.push('\n');
    }
    output.push_str(&color::header("Suggestions", color));
    output.push('\n');
    for diagnostic in &practice {
      append_text_diagnostic(&mut output, diagnostic, color);
    }
  }
  output
}

/// Score / reactivity footer for text reports (after streamed per-file findings).
#[must_use]
pub fn render_text_score_footer(summary: &ScanSummary, context: &ReportContext) -> String {
  let color = context.color;
  let mut output = String::new();
  output.push_str(&color::score_label(color));
  output.push_str(": ");
  output.push_str(&color::score_value(&summary.score.to_string(), color));
  output.push_str("/100 — ");
  output.push_str(&summary.files_scanned.to_string());
  output.push_str(" file(s), ");
  output.push_str(&summary.diagnostics.len().to_string());
  output.push_str(" finding(s)");
  if let Some(digest) = &context.reactivity {
    output.push_str(&render_reactivity_footer(digest, color));
  }
  output
}

fn append_text_diagnostic(output: &mut String, diagnostic: &Diagnostic, color: bool) {
  let location =
    format!("{}:{}:{}", diagnostic.file.display(), diagnostic.span.line, diagnostic.span.column);
  output.push_str(&color::location(&location, color));
  output.push_str("  ");
  output.push_str(&color::severity_label(
    diagnostic.severity,
    severity_name(diagnostic.severity),
    color,
  ));
  output.push_str("  ");
  output.push_str(&color::rule_id(&diagnostic.rule_id, color));
  output.push_str("  ");
  output.push_str(&diagnostic.message);
  output.push('\n');
  if let Some(help) = &diagnostic.help {
    output.push_str("  ");
    output.push_str(&color::help_prefix(color));
    output.push(' ');
    output.push_str(help);
    output.push('\n');
  }
  if let Some(recommendation) = &diagnostic.recommendation {
    output.push_str("  ");
    output.push_str(&color::recommend_prefix(color));
    output.push(' ');
    output.push_str(&recommendation.package);
    output.push(' ');
    output.push_str(&recommendation.export);
    output.push_str(" — ");
    output.push_str(&recommendation.docs_url);
    output.push('\n');
  }
}

const fn severity_name(severity: Severity) -> &'static str {
  match severity {
    Severity::Info => "info",
    Severity::Warning => "warning",
    Severity::Error => "error",
  }
}
