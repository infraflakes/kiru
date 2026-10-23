//! The server: the tree the editor's file belongs to, checked as it stands.
//!
//! A file belongs to the nearest `kiru.toml` above it, and the tree that
//! manifest names is analyzed as a whole: every body is linked and checked, so
//! a fact about one file never depends on which file the editor is looking at.
//! Only the open file's facts are published; there is no other mode. A file
//! with no project is not analyzed: the manifest is the total structure of the
//! codebase, and a tree is never inferred from a path.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::{
    Diagnostic as ProtocolDiagnostic, DiagnosticRelatedInformation, DiagnosticSeverity,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, InitializeParams, InitializeResult, InitializedParams, Location,
    MessageType, PositionEncodingKind, SemanticTokensFullOptions, SemanticTokensOptions,
    SemanticTokensParams, SemanticTokensResult, SemanticTokensServerCapabilities,
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
};
use tower_lsp_server::{Client, LanguageServer};

use crate::compiler::{Diagnostic, Severity, analyze_tree, semantic_tokens};
use crate::project::{Project, find};
use crate::sync::MutexExt;

use super::OverlaySource;
use super::positions::{Encoding, span_to_range};
use super::tokens;

/// The server a client speaks to: the client handle, the encoding its
/// positions count in, and the open documents.
pub struct Backend {
    client: Client,
    encoding: OnceLock<Encoding>,
    /// The text of every document the client holds open, as it stands in the
    /// editor. A token request carries no text of its own.
    documents: Mutex<HashMap<PathBuf, String>>,
    /// The messages already shown, so a keystroke does not repeat one.
    silenced: Mutex<HashSet<(PathBuf, String)>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            encoding: OnceLock::new(),
            documents: Mutex::new(HashMap::new()),
            silenced: Mutex::new(HashSet::new()),
        }
    }

    /// The project the file belongs to: the nearest manifest's tree, when the
    /// file stands in it. There is no tree without a manifest.
    fn project(&self, path: &Path) -> std::result::Result<Project, String> {
        let manifest =
            find(path).ok_or_else(|| format!("no `kiru.toml` above `{}`", path.display()))?;
        let project = Project::load(&manifest).map_err(|error| error.to_string())?;
        if !path.starts_with(&project.root) {
            return Err(format!(
                "`{}` is outside the project root `{}`",
                path.display(),
                project.root.display()
            ));
        }
        Ok(project)
    }

    /// Keep the text a document notification carries, under its settled path.
    fn remember(&self, uri: &Uri, source: &str) {
        let Some(path) = uri.to_file_path() else {
            return;
        };
        self.documents
            .lock_unpoisoned()
            .insert(settle(&path), source.to_owned());
    }

    /// Every open buffer over the disk, so the tree the editor sees is the
    /// tree an analysis reads.
    fn overlay(&self) -> OverlaySource {
        let documents = self.documents.lock_unpoisoned();
        let mut overlay = OverlaySource::new();
        for (path, source) in documents.iter() {
            overlay.open(path.clone(), source.clone());
        }
        overlay
    }

    /// Analyze the tree the document belongs to and publish the open file's
    /// facts. The tree is checked whole; only the file the editor holds is
    /// published.
    async fn check(&self, uri: &Uri) {
        let Some(raw) = uri.to_file_path() else {
            return;
        };
        let path = settle(&raw);
        let Some(uri) = Uri::from_file_path(&path) else {
            return;
        };
        let project = match self.project(&path) {
            Ok(project) => project,
            Err(reason) => {
                self.inactive(&path, uri.clone(), reason).await;
                return;
            }
        };
        let encoding = self.encoding.get().copied().unwrap_or(Encoding::Utf16);
        let overlay = self.overlay();
        let reports = analyze_tree(&project, &overlay);
        let source = self
            .documents
            .lock_unpoisoned()
            .get(&path)
            .cloned()
            .or_else(|| std::fs::read_to_string(&path).ok())
            .unwrap_or_default();
        let diagnostics: Vec<ProtocolDiagnostic> = reports
            .iter()
            .find(|file| file.path == path)
            .map(|file| file.diagnostics.as_slice())
            .unwrap_or(&[])
            .iter()
            .map(|diagnostic| protocol_diagnostic(&source, &path, diagnostic, encoding))
            .collect();
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }

    /// Clear the open file and explain once why it is not analyzed.
    async fn inactive(&self, path: &Path, uri: Uri, reason: String) {
        self.client.publish_diagnostics(uri, Vec::new(), None).await;
        let first = self
            .silenced
            .lock_unpoisoned()
            .insert((path.to_path_buf(), reason.clone()));
        if first {
            self.client.show_message(MessageType::WARNING, reason).await;
        }
    }
}

impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let encoding = negotiated(&params);
        let _ = self.encoding.set(encoding);
        Ok(InitializeResult {
            server_info: None,
            offset_encoding: None,
            capabilities: ServerCapabilities {
                position_encoding: Some(match encoding {
                    Encoding::Utf8 => PositionEncodingKind::UTF8,
                    Encoding::Utf16 => PositionEncodingKind::UTF16,
                }),
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: tokens::legend(),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            ..Default::default()
                        },
                    ),
                ),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {}

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.remember(&params.text_document.uri, &params.text_document.text);
        self.check(&params.text_document.uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().last() {
            self.remember(&params.text_document.uri, &change.text);
            self.check(&params.text_document.uri).await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        if let Some(text) = params.text {
            self.remember(&params.text_document.uri, &text);
        }
        self.check(&params.text_document.uri).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        if let Some(path) = params.text_document.uri.to_file_path() {
            self.documents.lock_unpoisoned().remove(&settle(&path));
        }
        self.client
            .publish_diagnostics(params.text_document.uri, Vec::new(), None)
            .await;
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let Some(raw) = params.text_document.uri.to_file_path() else {
            return Ok(None);
        };
        let path = settle(&raw);
        let project = match self.project(&path) {
            Ok(project) => project,
            Err(_) => return Ok(None),
        };
        let source = self
            .documents
            .lock_unpoisoned()
            .get(&path)
            .cloned()
            .or_else(|| std::fs::read_to_string(&path).ok());
        let Some(source) = source else {
            return Ok(None);
        };
        let encoding = *self.encoding.get().unwrap_or(&Encoding::Utf16);
        let overlay = self.overlay();
        let tokens = semantic_tokens(&project, &path, &overlay);
        Ok(Some(SemanticTokensResult::Tokens(tokens::encode(
            &source, tokens, encoding,
        ))))
    }
}

/// Serve the protocol on standard input and output until the client exits.
pub fn serve() {
    let runtime = tokio::runtime::Runtime::new().expect("the server starts its runtime");
    runtime.block_on(async {
        let (service, socket) = tower_lsp_server::LspService::new(Backend::new);
        tower_lsp_server::Server::new(tokio::io::stdin(), tokio::io::stdout(), socket)
            .serve(service)
            .await;
    });
}

/// The encoding the client and the server share: UTF-8 when the client offers
/// it, else the protocol's own UTF-16.
fn negotiated(params: &InitializeParams) -> Encoding {
    let offered = params
        .capabilities
        .general
        .as_ref()
        .and_then(|general| general.position_encodings.as_ref());
    match offered {
        Some(list) if list.contains(&PositionEncodingKind::UTF8) => Encoding::Utf8,
        _ => Encoding::Utf16,
    }
}

/// The canonical path of a file the editor names. A file the disk does not
/// have yet is settled through its parent, so a new unsaved buffer still joins
/// the tree it stands in.
fn settle(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
            return path.to_path_buf();
        };
        parent
            .canonicalize()
            .map(|parent| parent.join(name))
            .unwrap_or_else(|_| path.to_path_buf())
    })
}

/// The diagnostic the protocol carries: the same message, its span as a
/// range, and the severity it was given. A related place in another file
/// names a file the client did not ask about, so only this file's places
/// travel.
fn protocol_diagnostic(
    source: &str,
    path: &Path,
    diagnostic: &Diagnostic,
    encoding: Encoding,
) -> ProtocolDiagnostic {
    let severity = match diagnostic.severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
    };
    let related: Vec<DiagnosticRelatedInformation> = diagnostic
        .related
        .iter()
        .filter(|related| related.path == path)
        .filter_map(|related| {
            let uri = Uri::from_file_path(path)?;
            Some(DiagnosticRelatedInformation {
                location: Location {
                    uri,
                    range: span_to_range(source, related.span, encoding),
                },
                message: related.message.clone(),
            })
        })
        .collect();
    ProtocolDiagnostic {
        range: span_to_range(source, diagnostic.span, encoding),
        severity: Some(severity),
        source: Some("kiru".to_owned()),
        message: diagnostic.message.clone(),
        related_information: (!related.is_empty()).then_some(related),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp_server::jsonrpc::Request;

    /// The service over a backend with no state but its encoding.
    fn service() -> tower_lsp_server::LspService<Backend> {
        let (service, _socket) = tower_lsp_server::LspService::new(Backend::new);
        service
    }

    /// Deliver one message and read the answer to a request.
    async fn call(
        service: &mut tower_lsp_server::LspService<Backend>,
        id: i64,
        method: &str,
        params: serde_json::Value,
    ) -> Option<serde_json::Value> {
        let request = Request::build(method.to_owned())
            .params(params)
            .id(id)
            .finish();
        let response = tower::Service::call(service, request)
            .await
            .expect("the service answers");
        response.map(|response| {
            response
                .result()
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        })
    }

    /// Deliver one notification and let the service handle it.
    async fn notify(
        service: &mut tower_lsp_server::LspService<Backend>,
        method: &str,
        params: serde_json::Value,
    ) {
        let request = Request::build(method.to_owned()).params(params).finish();
        tower::Service::call(service, request)
            .await
            .expect("the notification is handled");
    }

    /// A project on the disk: its manifest and its one source file.
    fn project(source: &str) -> tempfile::TempDir {
        let directory = tempfile::tempdir().expect("a temporary directory");
        std::fs::write(
            directory.path().join("kiru.toml"),
            "[project]\nroot = \".\"\noutput = \"main\"\n",
        )
        .expect("the manifest");
        std::fs::write(directory.path().join("main.kiru"), source).expect("the program");
        directory
    }

    #[tokio::test]
    async fn initialize_declares_diagnostics_and_the_semantic_legend() {
        let mut service = service();
        let answer = call(
            &mut service,
            1,
            "initialize",
            serde_json::json!({"capabilities": {"general": {"positionEncodings": ["utf-8"]}}}),
        )
        .await
        .expect("initialize answers");
        let capabilities = &answer["capabilities"];
        assert_eq!(capabilities["positionEncoding"], "utf-8");
        assert_eq!(
            capabilities["textDocumentSync"], 1,
            "the whole document arrives"
        );
        assert_eq!(capabilities["definitionProvider"], serde_json::Value::Null);
        assert_eq!(capabilities["hoverProvider"], serde_json::Value::Null);
        assert_eq!(capabilities["completionProvider"], serde_json::Value::Null);
        assert_eq!(
            capabilities["semanticTokensProvider"]["full"], true,
            "the server offers the whole document's tokens"
        );
        assert_eq!(
            capabilities["semanticTokensProvider"]["legend"]["tokenTypes"][1],
            "type"
        );
        assert_eq!(
            capabilities["semanticTokensProvider"]["legend"]["tokenModifiers"],
            serde_json::json!(["declaration"])
        );
    }

    #[tokio::test]
    async fn a_document_answers_with_its_tokens() {
        let directory = project("fn main() {};");
        let path = directory.path().join("main.kiru");
        let uri = Uri::from_file_path(&path).expect("the uri");
        let mut service = service();
        call(
            &mut service,
            1,
            "initialize",
            serde_json::json!({"capabilities": {"general": {"positionEncodings": ["utf-8"]}}}),
        )
        .await;
        notify(
            &mut service,
            "textDocument/didOpen",
            serde_json::json!({
                "textDocument": {
                    "uri": uri.as_str(),
                    "languageId": "kiru",
                    "version": 1,
                    "text": "fn main() {};",
                }
            }),
        )
        .await;
        let answer = call(
            &mut service,
            2,
            "textDocument/semanticTokens/full",
            serde_json::json!({"textDocument": {"uri": uri.as_str()}}),
        )
        .await
        .expect("the tokens answer");
        assert_eq!(
            answer["data"],
            serde_json::json!([
                0, 0, 2, 8, 0, // fn
                0, 3, 4, 2, 1, // main
                0, 4, 1, 11, 0, // (
                0, 1, 1, 11, 0, // )
                0, 2, 1, 11, 0, // {
                0, 1, 1, 11, 0, // }
                0, 1, 1, 11, 0, // ;
            ]),
            "the keyword, the declared function, and the punctuation"
        );
    }

    #[tokio::test]
    async fn a_file_outside_a_project_is_not_analyzed() {
        let uri = Uri::from_file_path("/single/file.kiru").expect("the uri");
        let mut service = service();
        notify(
            &mut service,
            "textDocument/didOpen",
            serde_json::json!({
                "textDocument": {
                    "uri": uri.as_str(),
                    "languageId": "kiru",
                    "version": 1,
                    "text": "fn main() { let unused: int = 1; };",
                }
            }),
        )
        .await;
        let answer = call(
            &mut service,
            1,
            "textDocument/semanticTokens/full",
            serde_json::json!({"textDocument": {"uri": uri.as_str()}}),
        )
        .await;
        assert_eq!(answer, Some(serde_json::Value::Null));
    }
}
