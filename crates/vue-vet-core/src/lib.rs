use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceSpan {
    pub offset: usize,
    pub length: usize,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Diagnostic {
    pub rule_id: String,
    pub category: String,
    pub severity: Severity,
    pub message: String,
    pub help: Option<String>,
    pub file: PathBuf,
    pub span: SourceSpan,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScanSummary {
    pub files_scanned: usize,
    pub diagnostics: Vec<Diagnostic>,
    pub score: u8,
}

impl ScanSummary {
    pub fn finish(mut self) -> Self {
        self.diagnostics.sort_by(|left, right| {
            (&left.file, left.span.offset, &left.rule_id)
                .cmp(&(&right.file, right.span.offset, &right.rule_id))
        });

        let penalty = self.diagnostics.iter().fold(0_u16, |total, diagnostic| {
            total
                + match diagnostic.severity {
                    Severity::Error => 10,
                    Severity::Warning => 3,
                    Severity::Info => 1,
                }
        });
        self.score = 100_u16.saturating_sub(penalty).try_into().unwrap_or(0);
        self
    }

    pub fn fails(&self, deny_warnings: bool) -> bool {
        self.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error
                || (deny_warnings && diagnostic.severity == Severity::Warning)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_is_deterministic_and_saturating() {
        let diagnostic = Diagnostic {
            rule_id: "test/rule".into(),
            category: "test".into(),
            severity: Severity::Warning,
            message: "test".into(),
            help: None,
            file: "Component.vue".into(),
            span: SourceSpan { offset: 0, length: 1, line: 1, column: 1 },
        };
        let summary = ScanSummary {
            files_scanned: 1,
            diagnostics: vec![diagnostic; 40],
            score: 100,
        }
        .finish();

        assert_eq!(summary.score, 0);
        assert!(summary.fails(true));
        assert!(!summary.fails(false));
    }
}

