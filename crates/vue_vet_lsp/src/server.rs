//! Stdio language server backed by [`vue_vet_session::ProjectSession`].
//!
//! Publishes diagnostics from on-disk files plus unsaved buffer overlays
//! (`textDocument/didOpen` / `didChange` / `didSave`), exposes explicitly
//! safe quick-fix code actions, and answers hover with `--explain-scope`
//! (`ScopeExplain`) for the caret byte. Overlay changes advance the session revision,
//! stale CPU work cooperatively cancels between phases, and a debounced
//! latest-wins gate admits only one blocking analysis at a time. The initialized
//! [`ProjectSession`] retains sources, facts, graph partitions, and reverse
//! dependencies across edits.

use std::{
  collections::HashMap,
  path::{Path, PathBuf},
  sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
  },
  time::Duration,
};

use tokio::sync::{Mutex, RwLock};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
  CodeActionParams, CodeActionProviderCapability, CodeActionResponse, DidChangeTextDocumentParams,
  DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams, Hover,
  HoverParams, HoverProviderCapability, InitializeParams, InitializeResult, InitializedParams,
  MessageType, ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind,
  Url,
};
use tower_lsp::{Client, LanguageServer};
use vue_vet_core::SourceContext;
use vue_vet_session::{
  AnalysisProduct, AnalysisSnapshot, ChangeSet, ProjectSession, SessionOptions,
};

use crate::convert::{
  SafeCodeActionRequest, explain_scope_query, hover_from_scope_explains, position_to_byte,
  safe_code_actions, to_lsp_diagnostic_with_index,
};

#[derive(Clone, Debug)]
pub struct Backend {
  client: Client,
  state: Arc<RwLock<ServerState>>,
  analysis_gate: Arc<Mutex<()>>,
  publish_revision: Arc<AtomicU64>,
}

#[derive(Debug, Default)]
struct ServerState {
  root: Option<PathBuf>,
  session: Option<Arc<ProjectSession>>,
  open: HashMap<Url, OpenDocument>,
}

#[derive(Clone, Debug)]
struct OpenDocument {
  path: PathBuf,
  /// Shared text + line index; rebuilt on each buffer update.
  context: SourceContext,
  version: i32,
  /// Bumped on every open/change/save publish request; stale analyses drop results.
  generation: u64,
}

impl OpenDocument {
  fn set_text(&mut self, text: impl Into<std::sync::Arc<str>>) {
    self.context = SourceContext::new(text);
  }
}

impl Backend {
  #[must_use]
  pub fn new(client: Client) -> Self {
    Self {
      client,
      state: Arc::new(RwLock::new(ServerState::default())),
      analysis_gate: Arc::new(Mutex::new(())),
      publish_revision: Arc::new(AtomicU64::new(0)),
    }
  }

  fn schedule_publish(&self) {
    let revision = self.publish_revision.fetch_add(1, Ordering::AcqRel).saturating_add(1);
    let backend = self.clone();
    tokio::spawn(async move {
      backend.publish_workspace(revision).await;
    });
  }

  async fn publish_workspace(&self, revision: u64) {
    tokio::time::sleep(Duration::from_millis(50)).await;
    if !self.publish_current(revision) {
      return;
    }
    let _gate = self.analysis_gate.lock().await;
    if !self.publish_current(revision) {
      return;
    }
    let session = {
      let state = self.state.read().await;
      state.session.clone()
    };
    let Some(session) = session else {
      return;
    };
    let Some(analysis) = self.analyze_open(Arc::clone(&session)).await else {
      return;
    };
    if !self.publish_current(revision) {
      return;
    }

    let documents = {
      let state = self.state.read().await;
      state.open.iter().map(|(uri, document)| (uri.clone(), document.clone())).collect::<Vec<_>>()
    };
    for (uri, document) in documents {
      if !self.publish_current(revision) {
        return;
      }
      let Ok(file_id) = session.file_id_for_path(&document.path) else {
        continue;
      };
      let Ok(file_diagnostics) = session.diagnostics_for(&file_id) else {
        continue;
      };
      let diagnostics = file_diagnostics
        .iter()
        .map(|diagnostic| {
          to_lsp_diagnostic_with_index(
            diagnostic,
            analysis.analyzed_files.as_ref(),
            Some(document.context.text()),
            Some(document.context.line_index()),
          )
        })
        .collect::<Vec<_>>();

      self.client.publish_diagnostics(uri, diagnostics, Some(document.version)).await;
    }
  }

  fn publish_current(&self, expected: u64) -> bool {
    self.publish_revision.load(Ordering::Acquire) == expected
  }

  async fn analyze_open(&self, session: Arc<ProjectSession>) -> Option<AnalysisSnapshot> {
    // LSP only publishes diagnostics; skip materializing the full graph DTO.
    let result = tokio::task::spawn_blocking(move || {
      session.analyze_affected_product(AnalysisProduct::DiagnosticsOnly)
    })
    .await;
    let analysis = match result {
      Ok(Ok(analysis)) => analysis,
      Ok(Err(error)) if error.is_cancelled() => return None,
      Ok(Err(error)) => {
        self
          .client
          .log_message(MessageType::ERROR, format!("vue-vet analyze failed: {error}"))
          .await;
        return None;
      }
      Err(error) => {
        self
          .client
          .log_message(MessageType::ERROR, format!("vue-vet analysis worker failed: {error}"))
          .await;
        return None;
      }
    };
    Some(analysis)
  }
}

/// Returns true when `current` still matches the generation captured before analyze.
#[must_use]
pub const fn is_current_generation(current: Option<u64>, expected: u64) -> bool {
  matches!(current, Some(value) if value == expected)
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

    let session = ProjectSession::open(SessionOptions {
      root: root.clone(),
      config_path: None,
      cache_dir: None,
      no_cache: true,
      threads: None,
    })
    .map_err(|error| tower_lsp::jsonrpc::Error::invalid_params(error.to_string()))?;
    let mut state = self.state.write().await;
    state.root = Some(root);
    state.session = Some(Arc::new(session));
    drop(state);

    Ok(InitializeResult {
      capabilities: ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
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
        "vue-vet LSP ready (overlays + safe quick-fix + explain-scope hover)",
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
    {
      let mut state = self.state.write().await;
      let root = state.root.clone().unwrap_or_else(|| parent_or_cwd(&path));
      state.root.get_or_insert(root);
      state.open.insert(
        uri.clone(),
        OpenDocument {
          path,
          context: SourceContext::new(params.text_document.text),
          version: params.text_document.version,
          generation: 1,
        },
      );
    }
    let session = self.state.read().await.session.clone();
    if let Some(session) = session
      && let Some(document) = self.state.read().await.open.get(&uri).cloned()
      && let Err(error) =
        session.apply_changes(ChangeSet::upsert(document.path, document.context.text().to_owned()))
    {
      self
        .client
        .log_message(MessageType::ERROR, format!("vue-vet overlay update failed: {error}"))
        .await;
      return;
    }
    self.schedule_publish();
  }

  async fn did_change(&self, params: DidChangeTextDocumentParams) {
    let uri = params.text_document.uri.clone();
    let Some(text) = full_change_text(&params) else {
      return;
    };
    let (session, path, text) = {
      let mut state = self.state.write().await;
      let session = state.session.clone();
      let Some(doc) = state.open.get_mut(&uri) else {
        return;
      };
      doc.set_text(text);
      doc.version = params.text_document.version;
      doc.generation = doc.generation.saturating_add(1);
      let path = doc.path.clone();
      let text = doc.context.text().to_owned();
      drop(state);
      (session, path, text)
    };
    if let Some(session) = session
      && let Err(error) = session.apply_changes(ChangeSet::upsert(path, text))
    {
      self
        .client
        .log_message(MessageType::ERROR, format!("vue-vet overlay update failed: {error}"))
        .await;
      return;
    }
    self.schedule_publish();
  }

  async fn did_save(&self, params: DidSaveTextDocumentParams) {
    let uri = params.text_document.uri;
    let (session, path, text) = {
      let mut state = self.state.write().await;
      let session = state.session.clone();
      let Some(doc) = state.open.get_mut(&uri) else {
        return;
      };
      if let Some(text) = params.text {
        doc.set_text(text);
      } else if let Ok(disk) = std::fs::read_to_string(&doc.path) {
        doc.set_text(disk);
      }
      doc.generation = doc.generation.saturating_add(1);
      let path = doc.path.clone();
      let text = doc.context.text().to_owned();
      drop(state);
      (session, path, text)
    };
    if let Some(session) = session
      && let Err(error) = session.apply_changes(ChangeSet::upsert(path, text))
    {
      self
        .client
        .log_message(MessageType::ERROR, format!("vue-vet overlay update failed: {error}"))
        .await;
      return;
    }
    self.schedule_publish();
  }

  async fn did_close(&self, params: DidCloseTextDocumentParams) {
    let uri = params.text_document.uri;
    let (session, document) = {
      let mut state = self.state.write().await;
      (state.session.clone(), state.open.remove(&uri))
    };
    if let (Some(session), Some(document)) = (session, document) {
      let _ignored = session.apply_changes(ChangeSet::remove(document.path));
    }
    self.client.publish_diagnostics(uri, Vec::new(), None).await;
    self.schedule_publish();
  }

  async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
    let uri = params.text_document.uri;
    let (path, text, version, session) = {
      let state = self.state.read().await;
      let Some(doc) = state.open.get(&uri) else {
        return Ok(None);
      };
      let Some(session) = state.session.clone() else {
        return Ok(None);
      };
      let snapshot = (doc.path.clone(), doc.context.text().to_owned(), doc.version, session);
      drop(state);
      snapshot
    };
    let Ok(file_id) = session.file_id_for_path(&path) else {
      return Ok(None);
    };
    let _gate = self.analysis_gate.lock().await;
    let Some(analysis) = self.analyze_open(Arc::clone(&session)).await else {
      return Ok(None);
    };
    let only = params.context.only.as_deref();
    let actions = safe_code_actions(
      &analysis.summary.diagnostics,
      &SafeCodeActionRequest {
        uri,
        version,
        source: &text,
        document_file_id: &file_id,
        analyzed_files: analysis.analyzed_files.as_ref(),
        range: params.range,
        only,
      },
    );
    Ok(Some(actions).filter(|actions| !actions.is_empty()))
  }

  async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    let (path, text, line_index, session) = {
      let state = self.state.read().await;
      let Some(doc) = state.open.get(&uri) else {
        return Ok(None);
      };
      let Some(session) = state.session.clone() else {
        return Ok(None);
      };
      let snapshot =
        (doc.path.clone(), doc.context.text().to_owned(), doc.context.line_index_arc(), session);
      drop(state);
      snapshot
    };
    let Some(offset) = position_to_byte(&text, line_index.as_ref(), position) else {
      return Ok(None);
    };
    let Ok(file_id) = session.file_id_for_path(&path) else {
      return Ok(None);
    };
    let query = explain_scope_query(&file_id, offset);
    let _gate = self.analysis_gate.lock().await;
    let result = tokio::task::spawn_blocking(move || session.explain_scope(&query)).await;
    let explains = match result {
      Ok(Ok((explains, _))) => explains,
      Ok(Err(_)) => return Ok(None),
      Err(error) => {
        self
          .client
          .log_message(MessageType::ERROR, format!("vue-vet hover worker failed: {error}"))
          .await;
        return Ok(None);
      }
    };
    Ok(hover_from_scope_explains(&explains, &text, Some(line_index.as_ref())))
  }
}

fn parent_or_cwd(path: &Path) -> PathBuf {
  path.parent().map_or_else(
    || std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    Path::to_path_buf,
  )
}

#[cfg(test)]
mod tests {
  use super::is_current_generation;

  #[test]
  fn drops_stale_generations() {
    assert!(is_current_generation(Some(3), 3));
    assert!(!is_current_generation(Some(4), 3));
    assert!(!is_current_generation(None, 3));
  }
}
