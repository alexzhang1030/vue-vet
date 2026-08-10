//! Bundler-grade import resolution via `oxc_resolver` (Rolldown / enhanced-resolve).

use std::{
  collections::BTreeSet,
  path::{Component, Path, PathBuf},
};

use oxc_resolver::{
  AliasValue, ResolveError, ResolveOptions, Resolver, TsconfigDiscovery, TsconfigOptions,
  TsconfigReferences,
};

/// Pinned `oxc_resolver` crate version hashed into the content-addressed cache key.
pub const OXC_RESOLVER_VERSION: &str = "11.21.0";

pub enum Resolution {
  /// Path relative to the project root and present in the scanned file set.
  File(String),
  /// Outside the scanned set (including `node_modules`).
  ///
  /// `resolved_path` is the absolute filesystem path when `oxc_resolver` succeeded.
  /// Quiet virtuals (`#imports`, `node:…`, styles) leave it `None`.
  External {
    package: String,
    resolved_path: Option<PathBuf>,
  },
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
      return Resolution::External { package: specifier.into(), resolved_path: None };
    }
    let importer_path = absolute_under_root(&self.root, importer);
    match self.resolver.resolve_file(&importer_path, specifier) {
      Ok(resolved) => {
        classify_resolved(&self.root, resolved.full_path().as_path(), specifier, known)
      }
      // Bare `fs` / `path` / `fs/promises` (and `node:` forms) when `builtin_modules` is on.
      Err(ResolveError::Builtin { .. }) => {
        Resolution::External { package: specifier.into(), resolved_path: None }
      }
      // Nuxt virtuals (`#components`, `#build-info`, …) that fail resolve stay quiet —
      // they are not project source modules. Successful `#app/…` path mappings still resolve.
      Err(_) if specifier.starts_with('#') => {
        Resolution::External { package: specifier.into(), resolved_path: None }
      }
      Err(_) => Resolution::Unresolved,
    }
  }

  /// Resolve a specifier from an absolute importer path (external follow).
  pub fn resolve_from_absolute(&self, importer_absolute: &Path, specifier: &str) -> Resolution {
    if specifier == "#imports" || is_quiet_external_specifier(specifier) {
      return Resolution::External { package: specifier.into(), resolved_path: None };
    }
    match self.resolver.resolve_file(importer_absolute, specifier) {
      Ok(resolved) => {
        let absolute = resolved.full_path();
        Resolution::External { package: specifier.into(), resolved_path: Some(absolute) }
      }
      Err(ResolveError::Builtin { .. }) => {
        Resolution::External { package: specifier.into(), resolved_path: None }
      }
      Err(_) if specifier.starts_with('#') => {
        Resolution::External { package: specifier.into(), resolved_path: None }
      }
      Err(_) => Resolution::Unresolved,
    }
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
    // Surface bare Node builtins (`fs`, `path`, `fs/promises`) as `ResolveError::Builtin`
    // so project resolve can quiet them instead of emitting unresolved-import.
    builtin_modules: true,
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
    Some(_) | None => Resolution::External {
      package: specifier.into(),
      resolved_path: Some(absolute.to_path_buf()),
    },
  }
}

/// Prefer a companion `.d.ts` / `.d.mts` / `.d.cts` next to a resolved JS module.
///
/// When types live in a separate tree (`exports["."].types` → `dist/types/…`
/// while `import` → `dist/index.js`), also remap the package root JS entry via
/// `package.json` (`types` / `typings` / `exports["."].types`). Relative chunk
/// follows still require a sibling declaration (vue-query style).
#[must_use]
pub fn prefer_types_declaration(path: &Path) -> PathBuf {
  let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
    return path.to_path_buf();
  };
  let stem = file_name
    .strip_suffix(".mjs")
    .or_else(|| file_name.strip_suffix(".cjs"))
    .or_else(|| file_name.strip_suffix(".js"));
  if let Some(stem) = stem {
    for suffix in [".d.ts", ".d.mts", ".d.cts"] {
      let candidate = path.with_file_name(format!("{stem}{suffix}"));
      if candidate.is_file() {
        return candidate;
      }
    }
  }
  if let Some(types) = package_root_types_for_js_entry(path) {
    return types;
  }
  path.to_path_buf()
}

/// `package.json` types entry when `path` is that package's root `import` / `main`.
fn package_root_types_for_js_entry(js_path: &Path) -> Option<PathBuf> {
  let mut dir = js_path.parent()?;
  loop {
    let package_json = dir.join("package.json");
    if package_json.is_file() {
      let text = std::fs::read_to_string(&package_json).ok()?;
      let value: serde_json::Value = serde_json::from_str(&text).ok()?;
      let import_rel = package_root_import_specifier(&value)?;
      let import_path = dir.join(import_rel);
      if !same_path(js_path, &import_path) {
        return None;
      }
      let types_rel = package_root_types_specifier(&value)?;
      let types_path = dir.join(types_rel);
      return types_path.is_file().then_some(types_path);
    }
    dir = dir.parent()?;
  }
}

fn package_root_import_specifier(value: &serde_json::Value) -> Option<String> {
  if let Some(import) = value.pointer("/exports/.").and_then(|entry| match entry {
    serde_json::Value::String(path) => Some(path.as_str()),
    serde_json::Value::Object(map) => map
      .get("import")
      .and_then(|import| match import {
        serde_json::Value::String(path) => Some(path.as_str()),
        serde_json::Value::Object(nested) => nested
          .get("default")
          .and_then(serde_json::Value::as_str)
          .or_else(|| nested.values().find_map(serde_json::Value::as_str)),
        _ => None,
      })
      .or_else(|| {
        map
          .get("default")
          .and_then(serde_json::Value::as_str)
          .or_else(|| map.get("module").and_then(serde_json::Value::as_str))
      }),
    _ => None,
  }) {
    return Some(import.to_owned());
  }
  value
    .get("module")
    .or_else(|| value.get("main"))
    .and_then(serde_json::Value::as_str)
    .map(str::to_owned)
}

fn package_root_types_specifier(value: &serde_json::Value) -> Option<String> {
  if let Some(types) = value.pointer("/exports/.").and_then(|entry| match entry {
    serde_json::Value::Object(map) => map.get("types").and_then(|types| match types {
      serde_json::Value::String(path) => Some(path.as_str()),
      serde_json::Value::Object(nested) => nested
        .get("default")
        .and_then(serde_json::Value::as_str)
        .or_else(|| nested.values().find_map(serde_json::Value::as_str)),
      _ => None,
    }),
    _ => None,
  }) {
    return Some(types.to_owned());
  }
  value
    .get("types")
    .or_else(|| value.get("typings"))
    .and_then(serde_json::Value::as_str)
    .map(str::to_owned)
}

fn same_path(left: &Path, right: &Path) -> bool {
  if left == right {
    return true;
  }
  match (left.canonicalize(), right.canonicalize()) {
    (Ok(left), Ok(right)) => left == right,
    _ => false,
  }
}

/// Language hint for Oxc from a filesystem path.
#[must_use]
pub fn language_for_path(path: &Path) -> &'static str {
  let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("");
  if name.ends_with(".d.ts") || name.ends_with(".d.mts") || name.ends_with(".d.cts") {
    return "d.ts";
  }
  match path.extension().and_then(|ext| ext.to_str()) {
    Some("tsx") => "tsx",
    Some("jsx") => "jsx",
    Some("ts" | "mts" | "cts") => "ts",
    _ => "js",
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
    ".nuxt/imports.d.ts",
    ".nuxt/types/imports.d.ts",
    "auto-imports.d.ts",
    "src/auto-imports.d.ts",
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
  use super::{is_quiet_external_specifier, normalize_project_root, prefer_types_declaration};
  use std::path::Path;

  #[test]
  #[expect(clippy::expect_used, reason = "unit test asserts temp fixture setup succeeds")]
  fn prefer_types_remaps_package_root_js_via_exports_types() {
    let dir = std::env::temp_dir().join(format!(
      "vue-vet-prefer-types-{}-{}",
      std::process::id(),
      std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
    ));
    drop(std::fs::remove_dir_all(&dir));
    std::fs::create_dir_all(dir.join("dist/types")).expect("types dir");
    std::fs::create_dir_all(dir.join("dist")).expect("dist dir");
    std::fs::write(
      dir.join("package.json"),
      r#"{"name":"ui","types":"./dist/types/index.d.ts","exports":{".":{"types":"./dist/types/index.d.ts","import":"./dist/index.js"}}}"#,
    )
    .expect("package.json");
    let js = dir.join("dist/index.js");
    let dts = dir.join("dist/types/index.d.ts");
    std::fs::write(&js, "export {}\n").expect("js");
    std::fs::write(&dts, "export {}\n").expect("dts");
    assert_eq!(prefer_types_declaration(&js), dts);
    // Relative chunks still need a sibling declaration — do not remap to package root types.
    let chunk = dir.join("dist/chunk.js");
    std::fs::write(&chunk, "export {}\n").expect("chunk");
    assert_eq!(prefer_types_declaration(&chunk), chunk);
    drop(std::fs::remove_dir_all(&dir));
  }

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
    // Bare builtins are quieted via `ResolveError::Builtin` (see `quiets_bare_node_builtins`),
    // not the early `is_quiet_external_specifier` path.
    assert!(!is_quiet_external_specifier("fs"));
    assert!(!is_quiet_external_specifier("path"));
    // Failed `#…` virtuals quiet after resolve; `#imports` is special-cased earlier.
    assert!(!is_quiet_external_specifier("#components"));
  }

  #[test]
  #[expect(clippy::expect_used, reason = "unit test asserts temp fixture setup succeeds")]
  fn quiets_bare_node_builtins() {
    use super::{ProjectResolver, Resolution};
    use std::collections::BTreeSet;

    let dir = std::env::temp_dir().join(format!("vue-vet-builtins-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let importer = dir.join("tool.ts");
    std::fs::write(&importer, "import fs from 'fs'\n").expect("write importer");
    let resolver = ProjectResolver::new(&dir);
    let known = BTreeSet::new();
    for specifier in ["fs", "path", "fs/promises", "path/posix", "node:fs"] {
      let resolution = resolver.resolve("tool.ts", specifier, &known);
      assert!(
        matches!(
          &resolution,
          Resolution::External { package, resolved_path: None } if package == specifier
        ),
        "expected quiet External for `{specifier}`"
      );
    }
    // Non-builtins still miss when nothing is installed.
    assert!(
      matches!(
        resolver.resolve("tool.ts", "definitely-not-a-package", &known),
        Resolution::Unresolved
      ),
      "unknown bare packages must stay unresolved"
    );
    for virtual_spec in ["#components", "#build-info", "#storage-config"] {
      assert!(
        matches!(
          resolver.resolve("tool.ts", virtual_spec, &known),
          Resolution::External { resolved_path: None, .. }
        ),
        "failed Nuxt virtual `{virtual_spec}` must quiet as External"
      );
    }
    drop(std::fs::remove_dir_all(dir));
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
