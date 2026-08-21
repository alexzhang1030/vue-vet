//! `--explain` / `--explain-scope` early-exit commands.
use std::process::ExitCode;

use vue_vet_reporters::{
  render_finding_explain_json, render_finding_explain_text, render_rule_explain_json,
  render_rule_explain_text, render_scope_explains_json, render_scope_explains_text,
};
use vue_vet_session::Explained;

use crate::report::operational_failure;
use crate::{Cli, OutputFormat, open_session};

#[expect(clippy::print_stderr, reason = "cache stats for finding explain belong on stderr")]
pub fn run_explain(cli: &Cli, target: &str) -> ExitCode {
  let (session, _, _) = match open_session(cli) {
    Ok(opened) => opened,
    Err(error) => return operational_failure(cli, &error),
  };
  let explained = match session.explain(target) {
    Ok(explained) => explained,
    Err(error) => return operational_failure(cli, &error.to_string()),
  };
  if cli.cache.cache_stats
    && let Explained::Finding { cache_status, .. } = &explained
  {
    eprintln!("vue-vet cache: {cache_status}");
  }
  let output = match (&cli.format, explained) {
    (OutputFormat::Text, Explained::Rule(explain)) => Ok(render_rule_explain_text(&explain)),
    (OutputFormat::Json, Explained::Rule(explain)) => render_rule_explain_json(&explain),
    (OutputFormat::Text, Explained::Finding { explain, .. }) => {
      Ok(render_finding_explain_text(&explain))
    }
    (OutputFormat::Json, Explained::Finding { explain, .. }) => {
      render_finding_explain_json(&explain)
    }
    (OutputFormat::Sarif | OutputFormat::Github, _) => {
      return operational_failure(cli, "--explain supports --format text or json only");
    }
  };
  print_explain(cli, output)
}

#[expect(clippy::print_stderr, reason = "cache stats for scope explain belong on stderr")]
pub fn run_explain_scope(cli: &Cli, query: &str) -> ExitCode {
  let (session, _, _) = match open_session(cli) {
    Ok(opened) => opened,
    Err(error) => return operational_failure(cli, &error),
  };
  let (explains, cache_status) = match session.explain_scope(query) {
    Ok(result) => result,
    Err(error) => return operational_failure(cli, &error.to_string()),
  };
  if cli.cache.cache_stats {
    eprintln!("vue-vet cache: {cache_status}");
  }
  let output = match cli.format {
    OutputFormat::Text => Ok(render_scope_explains_text(&explains)),
    OutputFormat::Json => render_scope_explains_json(&explains),
    OutputFormat::Sarif | OutputFormat::Github => {
      return operational_failure(cli, "--explain-scope supports --format text or json only");
    }
  };
  print_explain(cli, output)
}

#[expect(clippy::print_stdout, reason = "explain is an early-exit CLI surface")]
fn print_explain(cli: &Cli, output: Result<String, serde_json::Error>) -> ExitCode {
  match output {
    Ok(rendered) => {
      print!("{rendered}");
      ExitCode::SUCCESS
    }
    Err(error) => operational_failure(cli, &format!("failed to serialize explain output: {error}")),
  }
}
