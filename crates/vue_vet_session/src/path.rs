use std::path::{Component, Path, PathBuf};

use crate::SessionError;

/// Resolve `path` under `root`, rejecting `..` escape and absolute paths outside root.
///
/// Normalization is lexical (no filesystem access) so missing files can still be
/// validated for workspace containment before agent or fix surfaces touch them.
///
/// # Errors
///
/// Returns [`SessionError`] when the resolved path is outside `root`.
pub fn resolve_under_root(root: &Path, path: &Path) -> Result<PathBuf, SessionError> {
  let root = normalize_lexically(root);
  let candidate = if path.is_absolute() {
    normalize_lexically(path)
  } else {
    normalize_lexically(&root.join(path))
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
}
