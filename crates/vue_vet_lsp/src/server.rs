//! Stdio language server backed by [`vue_vet_session::ProjectSession`].
//!
//! This thin slice analyzes **on-disk** content (didOpen / didSave). Unsaved
//! buffer overlays and cancellation remain later issue #12 work.

use std::{
  collections::HashMap,
  path::{Path, PathBuf},
  sync::Arc,
};

use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
  DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
  InitializeParams, InitializeResult, InitializedParams, MessageType, ServerCapabilities,
  ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind, Url,
};
use tower_lsp::{Client, LanguageServer};
use vue_vet_session::{ProjectSession, SessionOptions};

use crate::convert::to_lsp_diagnostic;

#[derive(Debug)]
pub struct Backend {
  client: Client,
  state: Arc<RwLock<ServerState>>,
}

#[derive(Debug, Default)]
struct ServerState {
  root: Option<PathBuf>,
  /// Open document URIs we have published diagnostics for.
  open: HashMap<Url, PathBuf>,
}

impl Backend {
  #[must_use]
  pub fn new(client: Client) -> Self {
    Self { client, state: Arc::new(RwLock::new(ServerState::default())) }
  }

  async fn publish_for_path(&self, uri: Url, path: &Path, root: &Path) {
    let options = SessionOptions {
      root: root.to_path_buf(),
      config_path: None,
      cache_dir: None,
      no_cache: false,
      threads: None,
    };
    let session = match ProjectSession::open(options) {
      Ok(session) => session,
      Err(error) => {
        self
          .client
          .log_message(MessageType::ERROR, format!("vue-vet session failed: {error}"))
          .await;
        return;
      }
    };
    let snapshot = match session.analyze() {
      Ok(snapshot) => snapshot,
      Err(error) => {
        self
          .client
          .log_message(MessageType::ERROR, format!("vue-vet analyze failed: {error}"))
          .await;
        return;
      }
    };

    let source = std::fs::read_to_string(path).ok();
    let normalized_path = normalize_report_path(path, root);
    let diagnostics = snapshot
      .summary
      .diagnostics
      .iter()
      .filter(|diagnostic| {
        let file = diagnostic.file.to_string_lossy().replace('\\', "/");
        file == normalized_path
          || file.ends_with(&normalized_path)
          || normalized_path.ends_with(&file)
      })
      .map(|diagnostic| to_lsp_diagnostic(diagnostic, &snapshot.analyzed_files, source.as_deref()))
      .collect::<Vec<_>>();

    self.client.publish_diagnostics(uri, diagnostics, None).await;
  }
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
      .log_message(
        MessageType::INFO,
        "vue-vet LSP ready (disk diagnostics on open/save; overlays deferred)",
      )
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
    let root = {
      let mut state = self.state.write().await;
      let root = state.root.clone().unwrap_or_else(|| parent_or_cwd(&path));
      state.open.insert(uri.clone(), path.clone());
      root
    };
    self.publish_for_path(uri, &path, &root).await;
  }

  async fn did_save(&self, params: DidSaveTextDocumentParams) {
    let uri = params.text_document.uri;
    let path = {
      let state = self.state.read().await;
      state.open.get(&uri).cloned()
    };
    let Some(path) = path.or_else(|| uri.to_file_path().ok()) else {
      return;
    };
    let root = self.state.read().await.root.clone().unwrap_or_else(|| parent_or_cwd(&path));
    self.publish_for_path(uri, &path, &root).await;
  }

  async fn did_close(&self, params: DidCloseTextDocumentParams) {
    let uri = params.text_document.uri;
    self.state.write().await.open.remove(&uri);
    self.client.publish_diagnostics(uri, Vec::new(), None).await;
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
