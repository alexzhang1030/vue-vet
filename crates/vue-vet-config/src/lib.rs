use std::{collections::BTreeMap, path::Path};

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use vue_vet_core::{Confidence, Diagnostic, Severity, SourceSpan};

pub const CONFIG_FILE: &str = "vue-vet.toml";
pub const CONFIG_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Preset {
  #[default]
  Recommended,
  None,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleLevel {
  Off,
  Info,
  Warning,
  Error,
}

impl RuleLevel {
  const fn severity(self) -> Option<Severity> {
    match self {
      Self::Off => None,
      Self::Info => Some(Severity::Info),
      Self::Warning => Some(Severity::Warning),
      Self::Error => Some(Severity::Error),
    }
  }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
  pub version: u32,
  pub preset: Preset,
  pub include: Vec<String>,
  pub exclude: Vec<String>,
  pub rules: BTreeMap<String, RuleLevel>,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      version: CONFIG_VERSION,
      preset: Preset::Recommended,
      include: vec!["**/*.vue".into()],
      exclude: Vec::new(),
      rules: BTreeMap::new(),
    }
  }
}

#[derive(Debug, Error)]
pub enum ConfigError {
  #[error("line {line}: {message}")]
  Invalid { line: usize, message: String },
  #[error("unsupported configuration version {0}; expected {CONFIG_VERSION}")]
  UnsupportedVersion(u32),
  #[error("unknown rule `{0}`")]
  UnknownRule(String),
  #[error("invalid path glob `{pattern}`: {message}")]
  InvalidGlob { pattern: String, message: String },
}

impl Config {
  /// Parse the versioned, strict Vue Vet TOML subset.
  ///
  /// # Errors
  ///
  /// Returns a line-oriented error for unknown keys, sections, values, or
  /// malformed arrays, and rejects unsupported configuration versions.
  pub fn parse(source: &str) -> Result<Self, ConfigError> {
    let mut config = Self::default();
    let mut section = "root";
    for (index, original) in source.lines().enumerate() {
      let line_number = index.saturating_add(1);
      let line = strip_comment(original).trim();
      if line.is_empty() {
        continue;
      }
      if line.starts_with('[') {
        if line != "[rules]" {
          return invalid(line_number, format!("unknown section `{line}`"));
        }
        section = "rules";
        continue;
      }
      let Some((raw_key, raw_value)) = line.split_once('=') else {
        return invalid(line_number, "expected `key = value`".into());
      };
      let key = unquote(raw_key.trim())
        .map_err(|message| ConfigError::Invalid { line: line_number, message })?;
      let value = raw_value.trim();
      if section == "rules" {
        let level = parse_rule_level(value).ok_or_else(|| ConfigError::Invalid {
          line: line_number,
          message: format!("invalid level for rule `{key}`"),
        })?;
        config.rules.insert(key, level);
        continue;
      }
      match key.as_str() {
        "version" => {
          config.version = value.parse().map_err(|_| ConfigError::Invalid {
            line: line_number,
            message: "version must be an integer".into(),
          })?;
        }
        "preset" => {
          config.preset = match unquote(value).as_deref() {
            Ok("recommended") => Preset::Recommended,
            Ok("none") => Preset::None,
            _ => return invalid(line_number, "preset must be `recommended` or `none`".into()),
          };
        }
        "include" => config.include = parse_string_array(value, line_number)?,
        "exclude" => config.exclude = parse_string_array(value, line_number)?,
        _ => return invalid(line_number, format!("unknown key `{key}`")),
      }
    }
    if config.version != CONFIG_VERSION {
      return Err(ConfigError::UnsupportedVersion(config.version));
    }
    Ok(config)
  }

  /// Validate rule overrides against registry metadata.
  ///
  /// # Errors
  ///
  /// Returns [`ConfigError::UnknownRule`] for the first unknown stable rule ID.
  pub fn validate_rules<'a>(
    &self,
    known_rules: impl IntoIterator<Item = &'a str>,
  ) -> Result<(), ConfigError> {
    let known = known_rules.into_iter().collect::<Vec<_>>();
    if let Some(rule) = self.rules.keys().find(|rule| !known.contains(&rule.as_str())) {
      return Err(ConfigError::UnknownRule(rule.clone()));
    }
    Ok(())
  }

  #[must_use]
  pub fn apply(&self, diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    diagnostics
      .into_iter()
      .filter_map(|mut diagnostic| {
        let configured = self.rules.get(&diagnostic.rule_id).copied();
        if self.preset == Preset::None && configured.is_none() {
          return None;
        }
        if let Some(level) = configured {
          diagnostic.severity = level.severity()?;
        }
        Some(diagnostic)
      })
      .collect()
  }

  /// Compile include and exclude patterns once for a scan.
  ///
  /// # Errors
  ///
  /// Returns [`ConfigError::InvalidGlob`] when a configured pattern is invalid.
  pub fn path_filter(&self) -> Result<PathFilter, ConfigError> {
    Ok(PathFilter { include: build_globs(&self.include)?, exclude: build_globs(&self.exclude)? })
  }
}

pub struct PathFilter {
  include: GlobSet,
  exclude: GlobSet,
}

impl PathFilter {
  #[must_use]
  pub fn matches(&self, path: &Path) -> bool {
    let normalized = path.to_string_lossy().replace('\\', "/");
    self.include.is_match(&normalized) && !self.exclude.is_match(&normalized)
  }
}

#[derive(Clone, Debug)]
struct Suppression {
  line: usize,
  next_line: bool,
  enable: bool,
  rules: Vec<String>,
  offset: usize,
  used: bool,
}

#[must_use]
pub fn apply_suppressions(
  file: &Path,
  source: &str,
  diagnostics: Vec<Diagnostic>,
) -> Vec<Diagnostic> {
  let mut suppressions = parse_suppressions(source);
  let mut retained = Vec::new();
  for diagnostic in diagnostics {
    let mut active_suppression = None;
    let mut suppressed = false;
    for (index, suppression) in suppressions.iter_mut().enumerate() {
      if suppression.line > diagnostic.span.line {
        continue;
      }
      let applies_to_rule = suppression.rules.is_empty()
        || suppression.rules.iter().any(|rule| rule == &diagnostic.rule_id);
      if suppression.next_line {
        if suppression.line.saturating_add(1) == diagnostic.span.line && applies_to_rule {
          suppression.used = true;
          suppressed = true;
        }
      } else if applies_to_rule {
        active_suppression = (!suppression.enable).then_some(index);
      }
    }
    if !suppressed && let Some(index) = active_suppression {
      suppressed = true;
      if let Some(suppression) = suppressions.get_mut(index) {
        suppression.used = true;
      }
    }
    if !suppressed {
      retained.push(diagnostic);
    }
  }

  retained.extend(
    suppressions.into_iter().filter(|suppression| !suppression.enable && !suppression.used).map(
      |suppression| Diagnostic {
        rule_id: "vue-vet/config/unused-suppression".into(),
        category: "configuration".into(),
        severity: Severity::Warning,
        confidence: Some(Confidence::High),
        documentation: None,
        message: "suppression did not hide any diagnostic".into(),
        help: Some("Remove the suppression or correct its rule ID and scope.".into()),
        file: file.to_path_buf(),
        span: line_span(source, suppression.offset),
      },
    ),
  );
  retained
}

fn parse_suppressions(source: &str) -> Vec<Suppression> {
  let mut result = Vec::new();
  let mut offset = 0_usize;
  for (index, line) in source.lines().enumerate() {
    let marker = line.find("vue-vet-");
    let comment = marker.filter(|marker| {
      let prefix = line.get(..*marker).unwrap_or_default();
      prefix.contains("<!--") || prefix.contains("//") || prefix.contains("/*")
    });
    if let Some(marker) = comment {
      let command = line.get(marker..).unwrap_or_default();
      let (next_line, enable, rest) =
        if let Some(rest) = command.strip_prefix("vue-vet-disable-next-line") {
          (true, false, rest)
        } else if let Some(rest) = command.strip_prefix("vue-vet-disable") {
          (false, false, rest)
        } else if let Some(rest) = command.strip_prefix("vue-vet-enable") {
          (false, true, rest)
        } else {
          offset = offset.saturating_add(line.len()).saturating_add(1);
          continue;
        };
      let rules = rest
        .trim_matches(|character: char| character.is_whitespace() || matches!(character, '-' | '>'))
        .split(',')
        .map(str::trim)
        .filter(|rule| !rule.is_empty())
        .map(str::to_owned)
        .collect();
      result.push(Suppression {
        line: index.saturating_add(1),
        next_line,
        enable,
        rules,
        offset: offset.saturating_add(marker),
        used: false,
      });
    }
    offset = offset.saturating_add(line.len()).saturating_add(1);
  }
  result
}

fn line_span(source: &str, offset: usize) -> SourceSpan {
  let prefix = source.as_bytes().get(..offset.min(source.len())).unwrap_or(source.as_bytes());
  let line =
    prefix.iter().fold(1_usize, |line, byte| line.saturating_add(usize::from(*byte == b'\n')));
  let column = prefix
    .iter()
    .rposition(|byte| *byte == b'\n')
    .map_or_else(|| prefix.len().saturating_add(1), |newline| prefix.len().saturating_sub(newline));
  SourceSpan { offset, length: "vue-vet-disable".len(), line, column }
}

fn build_globs(patterns: &[String]) -> Result<GlobSet, ConfigError> {
  let mut builder = GlobSetBuilder::new();
  for pattern in patterns {
    let glob = Glob::new(pattern).map_err(|error| ConfigError::InvalidGlob {
      pattern: pattern.clone(),
      message: error.to_string(),
    })?;
    builder.add(glob);
  }
  builder.build().map_err(|error| ConfigError::InvalidGlob {
    pattern: "<set>".into(),
    message: error.to_string(),
  })
}

fn strip_comment(line: &str) -> &str {
  let mut quoted = false;
  let mut escaped = false;
  for (index, character) in line.char_indices() {
    if escaped {
      escaped = false;
    } else if quoted && character == '\\' {
      escaped = true;
    } else if character == '"' {
      quoted = !quoted;
    } else if !quoted && character == '#' {
      return line.get(..index).unwrap_or(line);
    }
  }
  line
}

fn unquote(value: &str) -> Result<String, String> {
  let value = value.trim();
  if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
    return value
      .get(1..value.len().saturating_sub(1))
      .map(str::to_owned)
      .ok_or_else(|| "invalid quoted string".into());
  }
  if value
    .chars()
    .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
  {
    return Ok(value.into());
  }
  Err("expected a quoted string".into())
}

fn parse_rule_level(value: &str) -> Option<RuleLevel> {
  match unquote(value).ok()?.as_str() {
    "off" => Some(RuleLevel::Off),
    "info" => Some(RuleLevel::Info),
    "warning" => Some(RuleLevel::Warning),
    "error" => Some(RuleLevel::Error),
    _ => None,
  }
}

fn parse_string_array(value: &str, line: usize) -> Result<Vec<String>, ConfigError> {
  let Some(inner) = value.strip_prefix('[').and_then(|value| value.strip_suffix(']')) else {
    return invalid(line, "expected an array of quoted strings".into());
  };
  if inner.trim().is_empty() {
    return Ok(Vec::new());
  }
  inner
    .split(',')
    .map(|item| unquote(item.trim()).map_err(|message| ConfigError::Invalid { line, message }))
    .collect()
}

const fn invalid<T>(line: usize, message: String) -> Result<T, ConfigError> {
  Err(ConfigError::Invalid { line, message })
}

#[cfg(test)]
mod tests {
  use super::*;

  fn diagnostic(rule_id: &str, line: usize) -> Diagnostic {
    Diagnostic {
      rule_id: rule_id.into(),
      category: "test".into(),
      severity: Severity::Warning,
      confidence: Some(Confidence::High),
      documentation: None,
      message: "test finding".into(),
      help: None,
      file: Path::new("App.vue").to_path_buf(),
      span: SourceSpan { offset: 0, length: 1, line, column: 1 },
    }
  }

  #[test]
  fn parses_strict_config_and_applies_rule_levels() {
    let config = Config::parse(
      r#"version = 1
preset = "recommended"
include = ["src/**/*.vue"]
exclude = ["src/generated/**"]

[rules]
"vue-vet/security/no-v-html" = "error"
"vue-vet/accessibility/no-autofocus" = "off"
"#,
    );
    assert!(config.is_ok(), "valid configuration must parse");
    let config = config.unwrap_or_default();
    assert_eq!(config.rules.len(), 2, "both rule overrides must be retained");
    let filter = config.path_filter();
    assert!(filter.is_ok(), "valid globs must compile");
    if let Ok(filter) = filter {
      assert!(filter.matches(Path::new("src/components/App.vue")));
      assert!(!filter.matches(Path::new("src/generated/App.vue")));
    }
    let applied = config.apply(vec![
      diagnostic("vue-vet/security/no-v-html", 1),
      diagnostic("vue-vet/accessibility/no-autofocus", 2),
    ]);
    assert_eq!(applied.len(), 1, "off rules must be removed");
    assert_eq!(
      applied.first().map(|diagnostic| diagnostic.severity),
      Some(Severity::Error),
      "configured severity must override the preset"
    );
  }

  #[test]
  fn rejects_unknown_fields_and_versions() {
    assert!(
      matches!(Config::parse("unknown = true"), Err(ConfigError::Invalid { .. })),
      "unknown configuration fields must be rejected"
    );
    assert!(matches!(Config::parse("version = 2"), Err(ConfigError::UnsupportedVersion(2))));
  }

  #[test]
  fn preserves_hashes_inside_quoted_globs() {
    let config = Config::parse("version = 1\ninclude = [\"src/#generated/*.vue\"] # comment");
    assert!(config.is_ok(), "a hash inside a quoted string is not a TOML comment");
  }

  #[test]
  fn reports_unused_suppressions() {
    let source = "<!-- vue-vet-disable-next-line vue-vet/security/no-v-html -->\n<div />";
    let diagnostics = apply_suppressions(Path::new("App.vue"), source, Vec::new());
    assert_eq!(diagnostics.len(), 1, "unused suppressions must be visible");
    assert_eq!(
      diagnostics.first().map(|diagnostic| diagnostic.rule_id.as_str()),
      Some("vue-vet/config/unused-suppression")
    );
  }

  #[test]
  fn block_suppressions_can_be_reenabled_for_one_rule() {
    let source = "<!-- vue-vet-disable -->\n<div />\n\
      <!-- vue-vet-enable vue-vet/security/no-v-html -->\n<div v-html=\"value\" />";
    let diagnostics = apply_suppressions(
      Path::new("App.vue"),
      source,
      vec![diagnostic("vue-vet/security/no-v-html", 4)],
    );
    assert_eq!(
      diagnostics.len(),
      2,
      "the finding and now-unused global suppression must both remain visible"
    );
  }
}
