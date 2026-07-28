//! Bundler-grade import resolution via `oxc_resolver` (Rolldown / enhanced-resolve).

use std::{
  collections::BTreeSet,
  path::{Component, Path, PathBuf},
};

use oxc_resolver::{
  AliasValue, ResolveOptions, Resolver, TsconfigDiscovery, TsconfigOptions, TsconfigReferences,
};

/// Pinned `oxc_resolver` crate version hashed into the content-addressed cache key.
pub const OXC_RESOLVER_VERSION: &str = "11.21.0";

pub enum Resolution {
  File(String),
  External(String),
  Unresolved,
}

pub struct ProjectResolver {
  root: PathBuf,
  resolver: Resolver,
}

impl std::fmt::Debug for ProjectResolver {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter.debug_struct("ProjectResolver").field("root", &self.root).finish_non_exhaustive()
  }
}

impl ProjectResolver {
  pub fn new(root: &Path) -> Self {
    // Relative roots like `.` break alias targets (`~` → ".") and tsconfig paths.
    // On Windows, also strip `\\?\` so aliases match oxc_resolver's non-verbatim paths.
    let root = normalize_project_root(root);
    let options = bundler_resolve_options(&root);
    Self { root, resolver: Resolver::new(options) }
  }

  pub fn resolve(&self, importer: &str, specifier: &str, known: &BTreeSet<String>) -> Resolution {
    if specifier == "#imports" || is_quiet_external_specifier(specifier) {
      return Resolution::External(specifier.into());
    }
    let importer_path = absolute_under_root(&self.root, importer);
    self
      .resolver
      .resolve_file(&importer_path, specifier)
      .map_or(Resolution::Unresolved, |resolved| {
        classify_resolved(&self.root, resolved.full_path().as_path(), specifier, known)
      })
  }
}

/// Specifiers Vue Vet does not treat as project JS/TS modules.
///
/// They become `ExternalImport` (quiet) instead of `unresolved-import`, matching
/// Vite/Nuxt reality: Node builtins, stylesheets, and common virtual modules are
/// not meant to resolve to scanned source files.
fn is_quiet_external_specifier(specifier: &str) -> bool {
  const STYLE_EXTS: &[&str] =
    &[".css", ".scss", ".sass", ".less", ".styl", ".stylus", ".pcss", ".sss"];
  if specifier.starts_with("node:")
    || specifier.starts_with("nodejs:")
    || specifier.starts_with("virtual:")
    || specifier.starts_with('\0')
  {
    return true;
  }
  let bare = specifier.split_once('?').map_or(specifier, |(path, _)| path);
  bare == "uno.css"
    || bare.ends_with("/auto-routes")
    || STYLE_EXTS.iter().any(|ext| bare.ends_with(ext))
}

fn bundler_resolve_options(root: &Path) -> ResolveOptions {
  let src = root.join("src");
  let yarn_pnp = root.join(".pnp.cjs").is_file() || root.join(".pnp.data.json").is_file();
  let tsconfig = if root.join(".nuxt/tsconfig.json").is_file() {
    Some(TsconfigDiscovery::Manual(TsconfigOptions {
      config_file: root.join(".nuxt/tsconfig.json"),
      references: TsconfigReferences::Auto,
    }))
  } else {
    Some(TsconfigDiscovery::Auto)
  };

  ResolveOptions {
    cwd: Some(root.to_path_buf()),
    tsconfig,
    alias: vec![
      ("@".into(), vec![AliasValue::Path(path_string(&src))]),
      ("~".into(), vec![AliasValue::Path(path_string(root))]),
    ],
    alias_fields: vec![vec!["browser".into()]],
    condition_names: vec!["import".into(), "module".into(), "browser".into(), "default".into()],
    exports_fields: vec![vec!["exports".into()]],
    imports_fields: vec![vec!["imports".into()]],
    extension_alias: vec![
      (".js".into(), vec![".js".into(), ".ts".into(), ".tsx".into()]),
      (".jsx".into(), vec![".jsx".into(), ".ts".into(), ".tsx".into()]),
      (".mjs".into(), vec![".mjs".into(), ".mts".into()]),
      (".cjs".into(), vec![".cjs".into(), ".cts".into()]),
    ],
    extensions: [".vue", ".tsx", ".ts", ".jsx", ".js", ".mjs", ".cjs", ".json"]
      .into_iter()
      .map(str::to_owned)
      .collect(),
    main_fields: vec!["browser".into(), "module".into(), "main".into()],
    main_files: vec!["index".into()],
    modules: vec!["node_modules".into()],
    symlinks: true,
    node_path: false,
    builtin_modules: false,
    module_type: true,
    allow_package_exports_in_directory_resolve: true,
    yarn_pnp,
    ..ResolveOptions::default()
  }
}

fn classify_resolved(
  root: &Path,
  absolute: &Path,
  specifier: &str,
  known: &BTreeSet<String>,
) -> Resolution {
  match relativize(root, absolute) {
    Some(relative) if known.contains(&relative) => Resolution::File(relative),
    Some(_) | None => Resolution::External(specifier.into()),
  }
}

/// Absolutize + canonicalize a project root for resolver aliases and path joins.
///
/// Windows `fs::canonicalize` yields `\\?\C:\…` verbatim paths. `oxc_resolver`
/// and many walk results use ordinary `C:\…` forms; `Path::strip_prefix` then
/// fails and Nuxt `~/…` / `@/…` imports become unresolved. Strip compatible
/// verbatim prefixes after canonicalize so both sides share one representation.
#[must_use]
pub fn normalize_project_root(root: &Path) -> PathBuf {
  let absolute = if root.is_absolute() {
    root.to_path_buf()
  } else {
    std::env::current_dir().map_or_else(|_| root.to_path_buf(), |cwd| cwd.join(root))
  };
  let canonical = absolute.canonicalize().unwrap_or(absolute);
  strip_verbatim_prefix(canonical)
}

fn absolute_under_root(root: &Path, logical: &str) -> PathBuf {
  let path = Path::new(logical);
  if path.is_absolute() { strip_verbatim_prefix(path.to_path_buf()) } else { root.join(path) }
}

fn relativize(root: &Path, absolute: &Path) -> Option<String> {
  let root = strip_verbatim_prefix(root.canonicalize().unwrap_or_else(|_| root.to_path_buf()));
  let absolute =
    strip_verbatim_prefix(absolute.canonicalize().unwrap_or_else(|_| absolute.to_path_buf()));
  absolute.strip_prefix(root).ok().map(normalized_path)
}

#[cfg_attr(
  not(windows),
  expect(clippy::missing_const_for_fn, reason = "Windows branch is not const")
)]
fn strip_verbatim_prefix(path: PathBuf) -> PathBuf {
  #[cfg(windows)]
  {
    // `\\?\C:\foo` → `C:\foo`. Leave `\\?\UNC\…` alone — those need the prefix.
    const VERBATIM: &str = r"\\?\";
    let lossy = path.to_string_lossy();
    if let Some(rest) = lossy.strip_prefix(VERBATIM) {
      let bytes = rest.as_bytes();
      if bytes.len() >= 2 && bytes[1] == b':' {
        return PathBuf::from(rest);
      }
    }
    path
  }
  #[cfg(not(windows))]
  {
    path
  }
}

fn path_string(path: &Path) -> String {
  path.to_string_lossy().into_owned()
}

pub fn normalized_path(path: &Path) -> String {
  let mut parts = Vec::new();
  for component in path.components() {
    match component {
      Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
      Component::ParentDir => {
        parts.pop();
      }
      Component::CurDir | Component::RootDir | Component::Prefix(_) => {}
    }
  }
  parts.join("/")
}

/// Repository-relative paths that affect bundler resolution and must join cache keys.
#[must_use]
pub fn resolver_config_inputs(root: &Path) -> Vec<String> {
  const CANDIDATES: &[&str] = &[
    "package.json",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "bun.lock",
    "bun.lockb",
    "tsconfig.json",
    "tsconfig.app.json",
    "tsconfig.node.json",
    ".nuxt/tsconfig.json",
    ".nuxt/components.d.ts",
    ".nuxt/types/components.d.ts",
  ];
  let mut inputs = Vec::new();
  for candidate in CANDIDATES {
    if root.join(candidate).is_file() {
      inputs.push((*candidate).to_owned());
    }
  }
  // Additional root-level tsconfig*.json files (deterministic order).
  if let Ok(entries) = std::fs::read_dir(root) {
    let mut extras = entries
      .filter_map(Result::ok)
      .filter(|entry| {
        entry.file_type().is_ok_and(|kind| !kind.is_dir())
          && entry.file_name().to_str().is_some_and(|name| {
            name.starts_with("tsconfig")
              && Path::new(name).extension().is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
          })
      })
      .filter_map(|entry| entry.file_name().into_string().ok())
      .filter(|name| !inputs.iter().any(|existing| existing == name))
      .collect::<Vec<_>>();
    extras.sort();
    inputs.extend(extras);
  }
  inputs
}

#[cfg(test)]
mod tests {
  use super::{is_quiet_external_specifier, normalize_project_root};
  use std::path::Path;

  #[test]
  fn quiets_node_builtins_styles_and_common_virtuals() {
    assert!(is_quiet_external_specifier("node:path"));
    assert!(is_quiet_external_specifier("nodejs:fs"));
    assert!(is_quiet_external_specifier("virtual:vue-router/auto-routes"));
    assert!(is_quiet_external_specifier("uno.css"));
    assert!(is_quiet_external_specifier("uno.css?v=1"));
    assert!(is_quiet_external_specifier("vue-router/auto-routes"));
    assert!(is_quiet_external_specifier("./theme.css"));
    assert!(is_quiet_external_specifier("~/assets/main.scss"));
    assert!(!is_quiet_external_specifier("vue"));
    assert!(!is_quiet_external_specifier("./Child.vue"));
    assert!(!is_quiet_external_specifier("#imports"));
  }

  #[test]
  fn normalize_project_root_absolutizes_dot() {
    let root = normalize_project_root(Path::new("."));
    assert!(root.is_absolute(), "dot roots must become absolute: {}", root.display());
    #[cfg(windows)]
    {
      let display = root.to_string_lossy();
      assert!(
        !display.starts_with(r"\\?\"),
        "Windows roots must not keep verbatim prefixes for aliases: {display}"
      );
    }
  }

  #[cfg(windows)]
  mod windows {
    use super::super::{normalize_project_root, relativize, strip_verbatim_prefix};
    use std::path::PathBuf;

    #[test]
    fn strip_verbatim_disk_prefix_keeps_unc() {
      assert_eq!(
        strip_verbatim_prefix(PathBuf::from(r"\\?\C:\project\app")),
        PathBuf::from(r"C:\project\app")
      );
      assert_eq!(
        strip_verbatim_prefix(PathBuf::from(r"\\?\UNC\server\share\app")),
        PathBuf::from(r"\\?\UNC\server\share\app")
      );
    }

    #[test]
    #[expect(clippy::expect_used, reason = "unit test asserts temp fixture setup succeeds")]
    fn relativize_accepts_mixed_verbatim_and_disk_paths() {
      let dir = std::env::temp_dir().join(format!("vue-vet-verbatim-{}", std::process::id()));
      std::fs::create_dir_all(dir.join("utils")).expect("temp dir");
      let file = dir.join("utils").join("contract.ts");
      std::fs::write(&file, "export {}\n").expect("write");
      let root = normalize_project_root(&dir);
      let disk_file = strip_verbatim_prefix(file);
      let verbatim = PathBuf::from(format!(r"\\?\{}", disk_file.display()));
      let relative = relativize(&root, &verbatim);
      assert_eq!(
        relative.as_deref(),
        Some("utils/contract.ts"),
        "verbatim resolved paths must relativize against simplified roots: root={} file={}",
        root.display(),
        verbatim.display()
      );
      let _ignored = std::fs::remove_dir_all(dir);
    }
  }
}
