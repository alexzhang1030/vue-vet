use std::{
  ffi::OsString,
  fs,
  path::{Component, Path, PathBuf},
};

use crate::SessionError;

/// Resolve `path` under `root`, rejecting `..` escape and absolute paths outside root.
///
/// Lexical normalization keeps missing paths valid. When the workspace exists,
/// the longest existing candidate prefix is canonicalized so platform aliases
/// such as macOS `/var` → `/private/var` and nested symlinks are compared against
/// the canonical workspace identity.
///
/// # Errors
///
/// Returns [`SessionError`] when the resolved path is outside `root`.
pub fn resolve_under_root(root: &Path, path: &Path) -> Result<PathBuf, SessionError> {
  let lexical_root = normalize_lexically(root);
  let canonical_root = fs::canonicalize(&lexical_root).ok().map(|path| normalize_lexically(&path));
  let root = canonical_root.clone().unwrap_or(lexical_root);
  let candidate = if path.is_absolute() {
    normalize_lexically(path)
  } else {
    normalize_lexically(&root.join(path))
  };
  let candidate = if canonical_root.is_some() {
    canonicalize_existing_prefix(&candidate).unwrap_or(candidate)
  } else {
    candidate
  };
  if candidate == root || candidate.starts_with(&root) {
    return Ok(candidate);
  }
  Err(SessionError::message(format!(
    "path `{}` escapes the workspace root `{}`",
    path.display(),
    root.display()
  )))
}

fn canonicalize_existing_prefix(path: &Path) -> Option<PathBuf> {
  let mut prefix = path.to_path_buf();
  let mut suffix = Vec::<OsString>::new();
  loop {
    if let Ok(canonical) = fs::canonicalize(&prefix) {
      let mut resolved = normalize_lexically(&canonical);
      for component in suffix.iter().rev() {
        resolved.push(component);
      }
      return Some(normalize_lexically(&resolved));
    }
    let name = prefix.file_name()?.to_os_string();
    suffix.push(name);
    if !prefix.pop() {
      return None;
    }
  }
}

fn normalize_lexically(path: &Path) -> PathBuf {
  let mut parts = Vec::new();
  for component in path.components() {
    match component {
      Component::Prefix(prefix) => {
        parts.clear();
        parts.push(Component::Prefix(prefix));
      }
      Component::RootDir => {
        let keep_prefix =
          parts.first().copied().filter(|part| matches!(part, Component::Prefix(_)));
        parts.clear();
        if let Some(prefix) = keep_prefix {
          parts.push(prefix);
        }
        parts.push(Component::RootDir);
      }
      Component::CurDir => {}
      Component::ParentDir => match parts.last() {
        Some(Component::Normal(_)) => {
          parts.pop();
        }
        Some(
          Component::Prefix(_) | Component::RootDir | Component::CurDir | Component::ParentDir,
        )
        | None => {}
      },
      Component::Normal(name) => parts.push(Component::Normal(name)),
    }
  }
  let mut output = PathBuf::new();
  for component in parts {
    output.push(component.as_os_str());
  }
  if output.as_os_str().is_empty() { PathBuf::from(".") } else { output }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  #[expect(clippy::panic, reason = "path fixture assertions must fail the unit test")]
  fn accepts_relative_paths_inside_root() {
    let root = PathBuf::from("/workspace");
    let Ok(resolved) = resolve_under_root(&root, Path::new("src/App.vue")) else {
      panic!("inside root");
    };
    assert_eq!(resolved, PathBuf::from("/workspace/src/App.vue"));
  }

  #[test]
  #[expect(clippy::panic, reason = "path fixture assertions must fail the unit test")]
  fn rejects_parent_traversal() {
    let root = PathBuf::from("/workspace");
    let Err(error) = resolve_under_root(&root, Path::new("../secret")) else {
      panic!("escape");
    };
    assert!(error.to_string().contains("escapes"));
  }

  #[test]
  #[expect(clippy::panic, reason = "path fixture assertions must fail the unit test")]
  fn rejects_absolute_paths_outside_root() {
    let root = PathBuf::from("/workspace");
    let Err(error) = resolve_under_root(&root, Path::new("/etc/passwd")) else {
      panic!("outside");
    };
    assert!(error.to_string().contains("escapes"));
  }

  #[cfg(unix)]
  #[test]
  #[expect(clippy::panic, reason = "path fixture assertions must fail the unit test")]
  fn accepts_an_absolute_path_through_a_platform_symlink_alias() {
    use std::os::unix::fs::symlink;

    let fixture = std::env::temp_dir().join(format!("vue-vet-path-alias-{}", std::process::id()));
    let real_root = fixture.join("real");
    let alias_root = fixture.join("alias");
    fs::create_dir_all(&real_root).unwrap_or_else(|error| panic!("real root: {error}"));
    symlink(&real_root, &alias_root).unwrap_or_else(|error| panic!("alias root: {error}"));
    let canonical_root =
      fs::canonicalize(&real_root).unwrap_or_else(|error| panic!("canonical root: {error}"));
    let resolved = resolve_under_root(&canonical_root, &alias_root.join(".nuxt/components.d.ts"))
      .unwrap_or_else(|error| panic!("aliased missing child: {error}"));
    assert_eq!(resolved, canonical_root.join(".nuxt/components.d.ts"));
    fs::remove_file(alias_root).unwrap_or_else(|error| panic!("remove alias: {error}"));
    fs::remove_dir_all(fixture).unwrap_or_else(|error| panic!("remove fixture: {error}"));
  }

  #[cfg(unix)]
  #[test]
  #[expect(clippy::panic, reason = "path fixture assertions must fail the unit test")]
  fn rejects_a_missing_path_below_a_symlink_that_escapes_the_workspace() {
    use std::os::unix::fs::symlink;

    let fixture = std::env::temp_dir().join(format!("vue-vet-path-escape-{}", std::process::id()));
    let root = fixture.join("root");
    let outside = fixture.join("outside");
    fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace root: {error}"));
    fs::create_dir_all(&outside).unwrap_or_else(|error| panic!("outside root: {error}"));
    symlink(&outside, root.join("link")).unwrap_or_else(|error| panic!("escape symlink: {error}"));
    let canonical_root =
      fs::canonicalize(&root).unwrap_or_else(|error| panic!("canonical root: {error}"));
    let Err(error) = resolve_under_root(&canonical_root, &root.join("link/missing.vue")) else {
      panic!("a symlink escape must be rejected even when its child is missing");
    };
    assert!(error.to_string().contains("escapes"));
    fs::remove_dir_all(fixture).unwrap_or_else(|error| panic!("remove fixture: {error}"));
  }
}
