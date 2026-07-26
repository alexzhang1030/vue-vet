//! Stdio language server backed by [`vue_vet_session::ProjectSession`].
//!
//! Publishes diagnostics from on-disk files plus unsaved buffer overlays
//! (`textDocument/didOpen` / `didChange` / `didSave`) and exposes explicitly
//! safe quick-fix code actions. Request-level cancellation remains later
//! issue #12 work; overlapping overlay analyses are dropped via per-document
//! generation tokens.

use std::{
  collections::{BTreeMap, HashMap},
  path::{Path, PathBuf},
  sync::Arc,
};

use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
  CodeActionParams, CodeActionProviderCapability, CodeActionResponse, DidChangeTextDocumentParams,
  DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
  InitializeParams, InitializeResult, InitializedParams, MessageType, ServerCapabilities,
  ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind, Url,
};
use tower_lsp::{Client, LanguageServer};
use vue_vet_session::{AnalysisSnapshot, ProjectSession, SessionOptions};

use crate::convert::{SafeCodeActionRequest, safe_code_actions, to_lsp_diagnostic};

#[derive(Debug)]
pub struct Backend {
  client: Client,
  state: Arc<RwLock<ServerState>>,
}

#[derive(Debug, Default)]
struct ServerState {
  root: Option<PathBuf>,
  open: HashMap<Url, OpenDocument>,
}

#[derive(Clone, Debug)]
struct OpenDocument {
  path: PathBuf,
  text: String,
  version: i32,
  /// Bumped on every open/change/save publish request; stale analyses drop results.
  generation: u64,
}

impl Backend {
  #[must_use]
  pub fn new(client: Client) -> Self {
    Self { client, state: Arc::new(RwLock::new(ServerState::default())) }
  }

  async fn publish_uri(&self, uri: Url) {
    let snapshot_state = {
      let state = self.state.read().await;
      let Some(doc) = state.open.get(&uri) else {
        return;
      };
      let root = state.root.clone().unwrap_or_else(|| parent_or_cwd(&doc.path));
      let overlays = collect_overlays(&state.open);
      let request = PublishRequest {
        uri: uri.clone(),
        path: doc.path.clone(),
        text: doc.text.clone(),
        version: doc.version,
        generation: doc.generation,
        root,
        overlays,
      };
      drop(state);
      request
    };

    if !self.generation_current(&snapshot_state.uri, snapshot_state.generation).await {
      return;
    }
    let Some(analysis) =
      self.analyze_open(snapshot_state.root.clone(), &snapshot_state.overlays).await
    else {
      return;
    };
    if !self.generation_current(&snapshot_state.uri, snapshot_state.generation).await {
      return;
    }

    let normalized_path = normalize_report_path(&snapshot_state.path, &snapshot_state.root);
    let diagnostics = analysis
      .summary
      .diagnostics
      .iter()
      .filter(|diagnostic| {
        let file = diagnostic.file.to_string_lossy().replace('\\', "/");
        file == normalized_path
          || file.ends_with(&normalized_path)
          || normalized_path.ends_with(&file)
      })
      .map(|diagnostic| {
        to_lsp_diagnostic(diagnostic, &analysis.analyzed_files, Some(snapshot_state.text.as_str()))
      })
      .collect::<Vec<_>>();

    self
      .client
      .publish_diagnostics(snapshot_state.uri, diagnostics, Some(snapshot_state.version))
      .await;
  }

  async fn generation_current(&self, uri: &Url, expected: u64) -> bool {
    let state = self.state.read().await;
    is_current_generation(state.open.get(uri).map(|doc| doc.generation), expected)
  }

  async fn analyze_open(
    &self,
    root: PathBuf,
    overlays: &BTreeMap<PathBuf, String>,
  ) -> Option<AnalysisSnapshot> {
    let options =
      SessionOptions { root, config_path: None, cache_dir: None, no_cache: true, threads: None };
    let session = match ProjectSession::open(options) {
      Ok(session) => session,
      Err(error) => {
        self
          .client
          .log_message(MessageType::ERROR, format!("vue-vet session failed: {error}"))
          .await;
        return None;
      }
    };
    match session.analyze_with_overlays(overlays) {
      Ok(analysis) => Some(analysis),
      Err(error) => {
        self
          .client
          .log_message(MessageType::ERROR, format!("vue-vet analyze failed: {error}"))
          .await;
        None
      }
    }
  }
}

struct PublishRequest {
  uri: Url,
  path: PathBuf,
  text: String,
  version: i32,
  generation: u64,
  root: PathBuf,
  overlays: BTreeMap<PathBuf, String>,
}

/// Returns true when `current` still matches the generation captured before analyze.
#[must_use]
pub const fn is_current_generation(current: Option<u64>, expected: u64) -> bool {
  matches!(current, Some(value) if value == expected)
}

fn collect_overlays(open: &HashMap<Url, OpenDocument>) -> BTreeMap<PathBuf, String> {
  open.values().map(|doc| (doc.path.clone(), doc.text.clone())).collect()
}

fn full_change_text(params: &DidChangeTextDocumentParams) -> Option<String> {
  params.content_changes.last().map(|change| change.text.clone())
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
  async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
    let root = params
      .workspace_folders
      .as_ref()
      .and_then(|folders| folders.first())
      .and_then(|folder| folder.uri.to_file_path().ok())
      .or_else(|| params.root_uri.as_ref().and_then(|uri| uri.to_file_path().ok()))
      .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    self.state.write().await.root = Some(root);

    Ok(InitializeResult {
      capabilities: ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        ..ServerCapabilities::default()
      },
      server_info: Some(ServerInfo {
        name: "vue-vet".into(),
        version: Some(env!("CARGO_PKG_VERSION").into()),
      }),
    })
  }

  async fn initialized(&self, _: InitializedParams) {
    self
      .client
      .log_message(MessageType::INFO, "vue-vet LSP ready (overlays + safe quick-fix code actions)")
      .await;
  }

  async fn shutdown(&self) -> Result<()> {
    Ok(())
  }

  async fn did_open(&self, params: DidOpenTextDocumentParams) {
    let uri = params.text_document.uri;
    let Ok(path) = uri.to_file_path() else {
      return;
    };
    {
      let mut state = self.state.write().await;
      let root = state.root.clone().unwrap_or_else(|| parent_or_cwd(&path));
      state.root.get_or_insert(root);
      state.open.insert(
        uri.clone(),
        OpenDocument {
          path,
          text: params.text_document.text,
          version: params.text_document.version,
          generation: 1,
        },
      );
    }
    self.publish_uri(uri).await;
  }

  async fn did_change(&self, params: DidChangeTextDocumentParams) {
    let uri = params.text_document.uri.clone();
    let Some(text) = full_change_text(&params) else {
      return;
    };
    let mut state = self.state.write().await;
    let Some(doc) = state.open.get_mut(&uri) else {
      return;
    };
    doc.text = text;
    doc.version = params.text_document.version;
    doc.generation = doc.generation.saturating_add(1);
    drop(state);
    self.publish_uri(uri).await;
  }

  async fn did_save(&self, params: DidSaveTextDocumentParams) {
    let uri = params.text_document.uri;
    let mut state = self.state.write().await;
    let Some(doc) = state.open.get_mut(&uri) else {
      return;
    };
    if let Some(text) = params.text {
      doc.text = text;
    } else if let Ok(disk) = std::fs::read_to_string(&doc.path) {
      doc.text = disk;
    }
    doc.generation = doc.generation.saturating_add(1);
    drop(state);
    self.publish_uri(uri).await;
  }

  async fn did_close(&self, params: DidCloseTextDocumentParams) {
    let uri = params.text_document.uri;
    self.state.write().await.open.remove(&uri);
    self.client.publish_diagnostics(uri, Vec::new(), None).await;
  }

  async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
    let uri = params.text_document.uri;
    let (root, path, text, version, overlays) = {
      let state = self.state.read().await;
      let Some(doc) = state.open.get(&uri) else {
        return Ok(None);
      };
      let root = state.root.clone().unwrap_or_else(|| parent_or_cwd(&doc.path));
      let overlays = collect_overlays(&state.open);
      let snapshot = (root, doc.path.clone(), doc.text.clone(), doc.version, overlays);
      drop(state);
      snapshot
    };
    let Some(analysis) = self.analyze_open(root.clone(), &overlays).await else {
      return Ok(None);
    };
    let only = params.context.only.as_deref();
    let actions = safe_code_actions(
      &analysis.summary.diagnostics,
      &SafeCodeActionRequest {
        uri,
        version,
        source: &text,
        root: &root,
        document_path: &path,
        analyzed_files: &analysis.analyzed_files,
        range: params.range,
        only,
      },
    );
    Ok(Some(actions).filter(|actions| !actions.is_empty()))
  }
}

fn parent_or_cwd(path: &Path) -> PathBuf {
  path.parent().map_or_else(
    || std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    Path::to_path_buf,
  )
}

fn normalize_report_path(path: &Path, root: &Path) -> String {
  path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
  use super::{OpenDocument, collect_overlays, is_current_generation};
  use std::{collections::HashMap, path::PathBuf};
  use tower_lsp::lsp_types::Url;

  #[test]
  fn drops_stale_generations() {
    assert!(is_current_generation(Some(3), 3));
    assert!(!is_current_generation(Some(4), 3));
    assert!(!is_current_generation(None, 3));
  }

  #[test]
  #[expect(clippy::expect_used, reason = "unit test asserts Url::parse succeeds")]
  fn collects_all_open_overlays() {
    let mut open = HashMap::new();
    open.insert(
      Url::parse("file:///a.vue").expect("url"),
      OpenDocument { path: PathBuf::from("/a.vue"), text: "a".into(), version: 1, generation: 1 },
    );
    open.insert(
      Url::parse("file:///b.vue").expect("url"),
      OpenDocument { path: PathBuf::from("/b.vue"), text: "b".into(), version: 2, generation: 2 },
    );
    let overlays = collect_overlays(&open);
    assert_eq!(overlays.get(PathBuf::from("/a.vue").as_path()).map(String::as_str), Some("a"));
    assert_eq!(overlays.get(PathBuf::from("/b.vue").as_path()).map(String::as_str), Some("b"));
  }
}
