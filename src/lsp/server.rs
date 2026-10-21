//! The server: one session answers every request from the latest build.
//!
//! The backend holds no compiler logic. A notification edits the open buffers
//! and rebuilds; a request converts a position to an offset, asks the build,
//! and converts the answer back. The protocol types end at this module.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tokio::sync::Mutex;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::{
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams, CompletionResponse,
    Diagnostic as ProtocolDiagnostic, DiagnosticRelatedInformation, DiagnosticSeverity,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, DocumentChanges, DocumentSymbol, DocumentSymbolParams,
    DocumentSymbolResponse, GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverContents,
    HoverParams, HoverProviderCapability, InitializeParams, InitializeResult, InitializedParams,
    Location, MarkedString, OneOf, OptionalVersionedTextDocumentIdentifier, PositionEncodingKind,
    ReferenceParams, RenameParams, SemanticTokensFullOptions, SemanticTokensOptions,
    SemanticTokensParams, SemanticTokensResult, SemanticTokensServerCapabilities,
    ServerCapabilities, SymbolKind, TextDocumentEdit, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextEdit, Uri, WorkspaceEdit,
};
use tower_lsp_server::{Client, LanguageServer};

use crate::compiler::{
    CompletionKind, Diagnostic, FileDiagnostics, OutlineKind, OverlaySource, Severity, build,
};

use super::positions::{Encoding, position_to_offset, span_to_range};
use super::tokens;

/// The one checked program and the buffers the client holds open over it.
struct Session {
    /// The file whose directory tree is the program.
    entry: PathBuf,
    encoding: Encoding,
    open: HashMap<PathBuf, String>,
    build: Option<crate::compiler::Build>,
}

/// The server a client speaks to: one client handle and one session behind a
/// lock, rebuilt whole on every change.
pub struct Backend {
    client: Client,
    session: Mutex<Session>,
}

impl Backend {
    pub fn new(client: Client, entry: PathBuf) -> Self {
        Self {
            client,
            session: Mutex::new(Session {
                entry,
                encoding: Encoding::Utf16,
                open: HashMap::new(),
                build: None,
            }),
        }
    }

    /// Rebuild the program from the open buffers over the disk, and publish
    /// every file's diagnostics. A build that stops answers from the last
    /// good one; a first build that stops publishes the failure.
    async fn rebuild(&self) {
        let mut session = self.session.lock().await;
        let mut overlay = OverlaySource::new();
        for (path, source) in &session.open {
            overlay.open(path.clone(), source.clone());
        }
        let entry = session.entry.clone();
        match build(&entry, &overlay) {
            Ok(built) => {
                for file in built.diagnostics() {
                    self.publish(&session, &file).await;
                }
                session.build = Some(built);
            }
            Err(failure) => {
                for file in failure.diagnostics() {
                    self.publish(&session, &file).await;
                }
                session.build = None;
            }
        }
    }

    /// Publish one file's diagnostics. A file with none clears what the
    /// client was shown before.
    async fn publish(&self, session: &Session, file: &FileDiagnostics) {
        let Some(uri) = file_uri(&file.path) else {
            return;
        };
        let diagnostics = file
            .diagnostics
            .iter()
            .map(|diagnostic| protocol_diagnostic(session, &file.source, diagnostic))
            .collect();
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }

    /// The path and byte offset a position in one document stands at.
    fn position_of(
        &self,
        session: &Session,
        text_document_position: &tower_lsp_server::ls_types::TextDocumentPositionParams,
    ) -> Option<(PathBuf, usize)> {
        let path = file_path(&text_document_position.text_document.uri)?;
        let source = session.source_of(&path);
        let offset =
            position_to_offset(&source, &text_document_position.position, session.encoding);
        Some((path, offset))
    }
}

impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let encoding = negotiated(&params);
        {
            let mut session = self.session.lock().await;
            session.encoding = encoding;
            if let Some(root) = root_path(&params) {
                let entry = session.entry.clone();
                if !entry.is_absolute() {
                    session.entry = root.join(entry);
                }
            }
        }
        let position_encoding = match encoding {
            Encoding::Utf8 => Some(PositionEncodingKind::UTF8),
            Encoding::Utf16 => Some(PositionEncodingKind::UTF16),
        };
        Ok(InitializeResult {
            server_info: None,
            offset_encoding: None,
            capabilities: ServerCapabilities {
                position_encoding,
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                definition_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                references_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions::default()),
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

    async fn initialized(&self, _: InitializedParams) {
        self.rebuild().await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        {
            let mut session = self.session.lock().await;
            if let Some(path) = file_path(&params.text_document.uri) {
                session.open.insert(path, params.text_document.text.clone());
            }
        }
        self.rebuild().await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        {
            let mut session = self.session.lock().await;
            if let Some(path) = file_path(&params.text_document.uri)
                && let Some(change) = params.content_changes.into_iter().last()
            {
                session.open.insert(path, change.text);
            }
        }
        self.rebuild().await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        {
            let mut session = self.session.lock().await;
            if let (Some(path), Some(text)) = (file_path(&params.text_document.uri), params.text) {
                session.open.insert(path, text);
            }
        }
        self.rebuild().await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        {
            let mut session = self.session.lock().await;
            if let Some(path) = file_path(&params.text_document.uri) {
                session.open.remove(&path);
            }
        }
        self.rebuild().await;
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let session = self.session.lock().await;
        let Some((path, offset)) =
            self.position_of(&session, &params.text_document_position_params)
        else {
            return Ok(None);
        };
        let Some((target, span)) = session
            .build
            .as_ref()
            .and_then(|built| built.definition(&path, offset))
        else {
            return Ok(None);
        };
        let Some(uri) = file_uri(&target) else {
            return Ok(None);
        };
        Ok(Some(GotoDefinitionResponse::Scalar(Location {
            uri,
            range: span_to_range(&session.source_of(&target), span, session.encoding),
        })))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let session = self.session.lock().await;
        let Some((path, offset)) =
            self.position_of(&session, &params.text_document_position_params)
        else {
            return Ok(None);
        };
        let Some(text) = session
            .build
            .as_ref()
            .and_then(|built| built.hover(&path, offset))
        else {
            return Ok(None);
        };
        Ok(Some(Hover {
            contents: HoverContents::Scalar(MarkedString::String(text)),
            range: None,
        }))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let session = self.session.lock().await;
        let Some((path, offset)) = self.position_of(&session, &params.text_document_position)
        else {
            return Ok(None);
        };
        let include_declaration = params.context.include_declaration;
        let places = match &session.build {
            Some(built) => built.references(&path, offset, include_declaration),
            None => return Ok(None),
        };
        let locations: Vec<Location> = places
            .into_iter()
            .filter_map(|(target, span)| {
                Some(Location {
                    uri: file_uri(&target)?,
                    range: span_to_range(&session.source_of(&target), span, session.encoding),
                })
            })
            .collect();
        Ok(Some(locations))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let session = self.session.lock().await;
        let Some((path, offset)) = self.position_of(&session, &params.text_document_position)
        else {
            return Ok(None);
        };
        let answer = match &session.build {
            Some(built) => built.rename(&path, offset, &params.new_name),
            None => return Ok(None),
        };
        let edits = match answer {
            Ok(Some(rename)) => rename,
            Ok(None) => return Ok(None),
            Err(message) => {
                return Err(tower_lsp_server::jsonrpc::Error::invalid_params(message));
            }
        };
        let mut documents = Vec::new();
        for (target, spans) in edits.edits {
            let Some(uri) = file_uri(&target) else {
                continue;
            };
            let source = session.source_of(&target);
            let mut text_edits: Vec<TextEdit> = spans
                .into_iter()
                .map(|span| TextEdit {
                    range: span_to_range(&source, span, session.encoding),
                    new_text: params.new_name.clone(),
                })
                .collect();
            // The protocol applies edits in one document in order: the
            // deepest span first, so an earlier edit never moves a later one.
            text_edits.sort_by_key(|edit| (edit.range.start.line, edit.range.start.character));
            documents.push(TextDocumentEdit {
                text_document: OptionalVersionedTextDocumentIdentifier { uri, version: None },
                edits: text_edits.into_iter().map(OneOf::Left).collect(),
            });
        }
        Ok(Some(WorkspaceEdit {
            document_changes: Some(DocumentChanges::Edits(documents)),
            ..Default::default()
        }))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let session = self.session.lock().await;
        let Some(path) = file_path(&params.text_document.uri) else {
            return Ok(None);
        };
        let Some(built) = &session.build else {
            return Ok(None);
        };
        let source = session.source_of(&path);
        let symbols: Vec<DocumentSymbol> = built
            .outline(&path)
            .into_iter()
            .map(|symbol| document_symbol(&source, symbol, session.encoding))
            .collect();
        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let session = self.session.lock().await;
        let Some((path, offset)) = self.position_of(&session, &params.text_document_position)
        else {
            return Ok(None);
        };
        let Some(built) = &session.build else {
            return Ok(None);
        };
        let text = session.source_of(&path);
        let items: Vec<CompletionItem> = built
            .completions(&path, &text, offset)
            .into_iter()
            .map(|completion| CompletionItem {
                label: completion.name,
                kind: Some(completion_kind(completion.kind)),
                ..Default::default()
            })
            .collect();
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let session = self.session.lock().await;
        let Some(path) = file_path(&params.text_document.uri) else {
            return Ok(None);
        };
        let Some(built) = &session.build else {
            return Ok(None);
        };
        let source = session.source_of(&path);
        Ok(Some(SemanticTokensResult::Tokens(tokens::encoded(
            &source,
            built.semantic_tokens(&path),
            session.encoding,
        ))))
    }
}

impl Session {
    /// The text of one file as the client holds it, or as the last build read
    /// it.
    fn source_of(&self, path: &Path) -> String {
        if let Some(text) = self.open.get(path) {
            return text.clone();
        }
        self.build
            .as_ref()
            .and_then(|built| {
                built
                    .file_sources()
                    .into_iter()
                    .find(|(file, _)| file == path)
                    .map(|(_, text)| text)
            })
            .unwrap_or_default()
    }
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

/// The workspace root a relative entry resolves against.
fn root_path(params: &InitializeParams) -> Option<PathBuf> {
    if let Some(folders) = &params.workspace_folders
        && let Some(folder) = folders.first()
        && let Some(path) = folder.uri.to_file_path()
    {
        return Some(path.into_owned());
    }
    #[allow(deprecated)]
    // A client that names no workspace folder still names its root, and the
    // entry resolves against it: the deprecated field is the only one there.
    params
        .root_uri
        .as_ref()
        .and_then(|uri| uri.to_file_path())
        .map(|path| path.into_owned())
}

/// The file path a document uri names.
fn file_path(uri: &Uri) -> Option<PathBuf> {
    uri.to_file_path().map(|path| path.into_owned())
}

/// The uri a file path is published under.
fn file_uri(path: &Path) -> Option<Uri> {
    Uri::from_file_path(path)
}

/// The path the server speaks for. The entry is resolved before the service
/// starts, so this is the one argument the binary parses.
pub fn entry_from_arguments(arguments: impl IntoIterator<Item = String>) -> Option<PathBuf> {
    let mut arguments = arguments.into_iter();
    let first = arguments.next()?;
    if arguments.next().is_some() {
        return None;
    }
    Some(PathBuf::from(first))
}

/// The diagnostic the protocol carries: the same message, its span as a
/// range, and the severity it was given.
fn protocol_diagnostic(
    session: &Session,
    source: &str,
    diagnostic: &Diagnostic,
) -> ProtocolDiagnostic {
    let severity = match diagnostic.severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
    };
    let related: Vec<DiagnosticRelatedInformation> = diagnostic
        .related
        .iter()
        .filter_map(|related| {
            let uri = file_uri(&related.path)?;
            Some(DiagnosticRelatedInformation {
                location: Location {
                    uri,
                    range: span_to_range(
                        &session.source_of(&related.path),
                        related.span,
                        session.encoding,
                    ),
                },
                message: related.message.clone(),
            })
        })
        .collect();
    ProtocolDiagnostic {
        range: span_to_range(source, diagnostic.span, session.encoding),
        severity: Some(severity),
        source: Some("kiru".to_owned()),
        message: diagnostic.message.clone(),
        related_information: (!related.is_empty()).then_some(related),
        ..Default::default()
    }
}

/// The protocol's symbol kind for one outlined declaration.
fn symbol_kind(kind: OutlineKind) -> SymbolKind {
    match kind {
        OutlineKind::Rec => SymbolKind::STRUCT,
        OutlineKind::Enum => SymbolKind::ENUM,
        OutlineKind::Function => SymbolKind::FUNCTION,
        OutlineKind::Method => SymbolKind::METHOD,
        OutlineKind::Value => SymbolKind::CONSTANT,
        OutlineKind::Field => SymbolKind::FIELD,
        OutlineKind::Variant => SymbolKind::ENUM_MEMBER,
    }
}

/// One outlined declaration with its children.
fn document_symbol(
    source: &str,
    symbol: crate::compiler::Outline,
    encoding: Encoding,
) -> DocumentSymbol {
    let range = span_to_range(source, symbol.span, encoding);
    DocumentSymbol {
        name: symbol.name,
        detail: None,
        kind: symbol_kind(symbol.kind),
        tags: None,
        #[allow(deprecated)]
        deprecated: None,
        range,
        selection_range: range,
        children: Some(
            symbol
                .children
                .into_iter()
                .map(|child| document_symbol(source, child, encoding))
                .collect(),
        ),
    }
}

/// The protocol's item kind for one completion.
fn completion_kind(kind: CompletionKind) -> CompletionItemKind {
    match kind {
        CompletionKind::Function => CompletionItemKind::FUNCTION,
        CompletionKind::Rec => CompletionItemKind::STRUCT,
        CompletionKind::Enum => CompletionItemKind::ENUM,
        CompletionKind::Namespace => CompletionItemKind::MODULE,
        CompletionKind::Variable => CompletionItemKind::VARIABLE,
        CompletionKind::Field => CompletionItemKind::FIELD,
        CompletionKind::Variant => CompletionItemKind::ENUM_MEMBER,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp_server::jsonrpc::Request;

    /// The service over a backend whose entry is `entry`.
    fn service(entry: &Path) -> tower_lsp_server::LspService<Backend> {
        let entry = entry.to_path_buf();
        let (service, _socket) =
            tower_lsp_server::LspService::new(move |client| Backend::new(client, entry.clone()));
        service
    }

    /// Call one request and read its answer, which the service types.
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

    #[tokio::test]
    async fn initialize_declares_every_capability() {
        let entry = std::env::temp_dir().join("kls-test-entry.kiru");
        let mut service = service(&entry);
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
        assert_eq!(capabilities["textDocumentSync"], 1);
        assert_eq!(capabilities["definitionProvider"], true);
        assert_eq!(capabilities["hoverProvider"], true);
        assert_eq!(capabilities["referencesProvider"], true);
        assert!(
            capabilities["renameProvider"].is_object() || capabilities["renameProvider"] == true
        );
        assert_eq!(capabilities["documentSymbolProvider"], true);
        assert!(capabilities["completionProvider"].is_object());
        assert!(capabilities["semanticTokensProvider"]["legend"]["tokenTypes"].is_array());
    }
}
