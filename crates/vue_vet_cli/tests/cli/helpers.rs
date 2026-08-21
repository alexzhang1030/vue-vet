use std::{
  path::{Path, PathBuf},
  process::Output,
  sync::atomic::{AtomicUsize, Ordering},
};

pub use serde_json::Value;
pub use std::{fs, process::Command};

pub static NEXT_TEMP_PROJECT: AtomicUsize = AtomicUsize::new(0);

pub struct TempProject {
  root: PathBuf,
}

impl TempProject {
  #[expect(clippy::panic, reason = "test setup failures must fail the integration test")]
  pub fn new(name: &str, source: &str) -> Self {
    let sequence = NEXT_TEMP_PROJECT.fetch_add(1, Ordering::Relaxed);
    let root = workspace_root()
      .join("target")
      .join(format!("test-{name}-{}-{sequence}", std::process::id()));
    let _ignored = fs::remove_dir_all(&root);
    if let Err(error) = fs::create_dir_all(&root) {
      panic!("failed to create temporary project {}: {error}", root.display());
    }
    let source_path = root.join("App.vue");
    if let Err(error) = fs::write(&source_path, source) {
      panic!("failed to write temporary source {}: {error}", source_path.display());
    }
    Self { root }
  }

  pub fn source_path(&self) -> PathBuf {
    self.root.join("App.vue")
  }

  pub fn root(&self) -> &Path {
    &self.root
  }

  #[expect(clippy::panic, reason = "test setup failures must fail the integration test")]
  pub fn write_source(&self, name: &str, source: &str) -> PathBuf {
    let path = self.root.join(name);
    if let Some(parent) = path.parent()
      && let Err(error) = fs::create_dir_all(parent)
    {
      panic!("failed to create {}: {error}", parent.display());
    }
    if let Err(error) = fs::write(&path, source) {
      panic!("failed to write temporary source {}: {error}", path.display());
    }
    path
  }
}

impl Drop for TempProject {
  fn drop(&mut self) {
    let _ignored = fs::remove_dir_all(&self.root);
  }
}

pub fn fixture(name: &str) -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(name)
}

pub fn workspace_root() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn collect_reference_sources(directory: &Path, sources: &mut Vec<PathBuf>) {
  let Ok(entries) = fs::read_dir(directory) else {
    return;
  };
  for entry in entries.flatten() {
    let path = entry.path();
    if path.is_dir() {
      collect_reference_sources(&path, sources);
    } else if matches!(
      path.extension().and_then(|extension| extension.to_str()),
      Some("vue" | "js" | "jsx" | "ts" | "tsx")
    ) {
      sources.push(path);
    }
  }
}

#[expect(clippy::panic, reason = "an unexpected process error must fail the integration test")]
pub fn run(arguments: &[&str]) -> Output {
  match Command::new(env!("CARGO_BIN_EXE_vue-vet")).args(arguments).output() {
    Ok(output) => output,
    Err(error) => panic!("failed to run vue-vet: {error}"),
  }
}

#[expect(clippy::panic, reason = "an unexpected process error must fail the integration test")]
pub fn run_from_workspace(arguments: &[&str]) -> Output {
  match Command::new(env!("CARGO_BIN_EXE_vue-vet"))
    .current_dir(workspace_root())
    .args(arguments)
    .output()
  {
    Ok(output) => output,
    Err(error) => panic!("failed to run vue-vet from the workspace root: {error}"),
  }
}
