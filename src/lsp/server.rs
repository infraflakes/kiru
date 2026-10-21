//! The server: the file the editor holds, checked as it stands.
//!
//! A request's scope is one file. The compiler's single-file check mounts the
//! library the file belongs to and reads the buffer, so a file is checked the
//! moment it opens, whatever its name, wherever it stands, and without a
//! program or an entry.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::{
    Diagnostic as ProtocolDiagnostic, DiagnosticRelatedInformation, DiagnosticSeverity,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, InitializeParams, InitializeResult, InitializedParams, Location,
    PositionEncodingKind, SemanticTokensFullOptions, SemanticTokensOptions, SemanticTokensParams,
    SemanticTokensResult, SemanticTokensServerCapabilities, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
};
use tower_lsp_server::{Client, LanguageServer};

use crate::compiler::{Diagnostic, STD_TREE, Severity, analyze_file, semantic_tokens};

use super::OverlaySource;
use super::positions::{Encoding, span_to_range};
use super::tokens;

/// The server a client speaks to: the client handle, and the encoding its
/// positions count in.
pub struct Backend {
    client: Client,
    encoding: OnceLock<Encoding>,
    /// The text of every document the client holds open, as it stands in the
    /// editor. A token request carries no text of its own.
    documents: Mutex<HashMap<PathBuf, String>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            encoding: OnceLock::new(),
            documents: Mutex::new(HashMap::new()),
        }
    }

    /// Keep the text a document notification carries.
    fn remember(&self, uri: &Uri, source: &str) {
        let Some(path) = uri.to_file_path() else {
            return;
        };
        self.documents
            .lock()
            .expect("the document map is not poisoned")
            .insert(path.into_owned(), source.to_owned());
    }

    /// Analyze one document as the client holds it and publish its facts.
    async fn check(&self, uri: Uri, source: &str) {
        let Some(path) = uri.to_file_path() else {
            return;
        };
        // A path that cannot be named as a uri earns no publication.
        let Some(uri) = Uri::from_file_path(&path) else {
            return;
        };
        let encoding = self.encoding.get().copied().unwrap_or(Encoding::Utf16);
        let mut overlay = OverlaySource::new();
        overlay.open(path.to_path_buf(), source.to_owned());
        let library = library_root(&path);
        let diagnostics = analyze_file(&path, &overlay, library)
            .iter()
            .map(|diagnostic| protocol_diagnostic(source, &path, diagnostic, encoding))
            .collect();
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
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
        self.check(params.text_document.uri, &params.text_document.text)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().last() {
            self.remember(&params.text_document.uri, &change.text);
            self.check(params.text_document.uri, &change.text).await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        if let Some(text) = params.text {
            self.remember(&params.text_document.uri, &text);
            self.check(params.text_document.uri, &text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        if let Some(path) = params.text_document.uri.to_file_path() {
            self.documents
                .lock()
                .expect("the document map is not poisoned")
                .remove(path.as_ref());
        }
        self.client
            .publish_diagnostics(params.text_document.uri, Vec::new(), None)
            .await;
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let Some(path) = params.text_document.uri.to_file_path() else {
            return Ok(None);
        };
        let Some(source) = self
            .documents
            .lock()
            .expect("the document map is not poisoned")
            .get(path.as_ref())
            .cloned()
        else {
            return Ok(None);
        };
        let encoding = *self.encoding.get().unwrap_or(&Encoding::Utf16);
        let mut overlay = OverlaySource::new();
        overlay.open(path.to_path_buf(), source.clone());
        let tokens = semantic_tokens(&path, &overlay, library_root(&path));
        Ok(Some(SemanticTokensResult::Tokens(tokens::encode(
            &source, tokens, encoding,
        ))))
    }
}

/// The library a file belongs to: a file under a directory named `std` is
/// analyzed in that library's tree; any other file stands in the root
/// namespace.
fn library_root(path: &Path) -> Option<&Path> {
    path.ancestors()
        .find(|ancestor| ancestor.file_name().is_some_and(|name| name == STD_TREE))
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
    use crate::compiler::VirtualSource;
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
        response.map(|response| response.result().cloned().expect("the answer is a result"))
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
        let uri = Uri::from_file_path("/single/tokens.kiru").expect("the uri");
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
    async fn a_document_is_checked_under_its_own_name() {
        let uri = Uri::from_file_path("/single/file.kiru").expect("the uri");
        let request = Request::build("textDocument/didOpen".to_owned())
            .params(serde_json::json!({
                "textDocument": {
                    "uri": uri.as_str(),
                    "languageId": "kiru",
                    "version": 1,
                    "text": "use std::text;\\nfn main() { let unused: int = 1; };",
                }
            }))
            .finish();
        let mut service = service();
        tower::Service::call(&mut service, request)
            .await
            .expect("the notification is handled");
    }

    #[test]
    fn the_real_library_file_analyzes_without_errors() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("std");
        let path = root.join("dynamic/vec.kiru");
        let source = std::fs::read_to_string(&path).expect("the library file is readable");
        let mut overlay = OverlaySource::new();
        overlay.open(path.clone(), source);
        let facts = analyze_file(&path, &overlay, Some(&root));
        assert!(
            facts
                .iter()
                .all(|diagnostic| !matches!(diagnostic.severity, Severity::Error)),
            "the library's own file analyzes without errors: {facts:?}"
        );
    }

    #[test]
    fn a_file_reports_its_own_warnings_only() {
        let path = Path::new("/virtual/main.kiru");
        let local = analyze_file(
            path,
            &VirtualSource::new(&[("main.kiru", "fn main() { let unused: int = 1; };")]),
            None,
        );
        assert_eq!(
            local.len(),
            1,
            "the file writes one unused local: {local:?}"
        );
        assert_eq!(local[0].message, "the local `unused` is never read");
        assert_eq!(local[0].path, path);

        let unused_use = analyze_file(
            path,
            &VirtualSource::new(&[("main.kiru", "use std::text;\nfn main() {};")]),
            None,
        );
        assert_eq!(
            unused_use.len(),
            1,
            "the file writes one unused use: {unused_use:?}"
        );
        assert_eq!(unused_use[0].message, "unused use `std::text`");
        assert_eq!(unused_use[0].path, path);
    }
}
