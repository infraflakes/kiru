//! The server: the tree the editor's file belongs to, checked as it stands.
//!
//! A file belongs to the nearest `kiru.toml` above it, and the tree that
//! manifest names is analyzed as a whole: every body is linked and checked, so
//! a fact about one file never depends on which file the editor is looking at.
//! The manifest's `[lsp] scope` decides whether one file or the whole tree is
//! published. A file under the compiler's own `std/` source tree is the one
//! special case: it is analyzed as the library, with no project and no entry.
//! A file with no project is not analyzed.

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

use crate::compiler::{
    Diagnostic, EMBEDDED, STD_TREE, Severity, Tree, analyze_tree, semantic_tokens,
};
use crate::project::{DiagnosticScope, Project, find};

use super::OverlaySource;
use super::positions::{Encoding, span_to_range};
use super::tokens;

/// The server a client speaks to: the client handle, the encoding its
/// positions count in, the open documents, and what it published where.
pub struct Backend {
    client: Client,
    encoding: OnceLock<Encoding>,
    /// The text of every document the client holds open, as it stands in the
    /// editor. A token request carries no text of its own.
    documents: Mutex<HashMap<PathBuf, String>>,
    /// The files published per context, so a file that grows clean is cleared.
    published: Mutex<HashMap<PathBuf, HashSet<PathBuf>>>,
    /// The messages already shown, so a keystroke does not repeat one.
    silenced: Mutex<HashSet<(PathBuf, String)>>,
}

/// Which tree a file belongs to, with the tree's own data.
enum Context {
    Program(Project),
    Library(PathBuf),
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            encoding: OnceLock::new(),
            documents: Mutex::new(HashMap::new()),
            published: Mutex::new(HashMap::new()),
            silenced: Mutex::new(HashSet::new()),
        }
    }

    /// The tree the file belongs to. A directory named `std` that holds the
    /// library's files is the library, not a project: it is the one tree
    /// analyzed without a manifest, and it is recognized by its contents so an
    /// installed binary needs no checkout path. Otherwise the nearest
    /// manifest's tree owns the file, and a file outside that tree has no tree
    /// at all.
    fn context(&self, path: &Path) -> std::result::Result<Context, String> {
        if let Some(root) = standard_library_root(path) {
            return Ok(Context::Library(root));
        }
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
        Ok(Context::Program(project))
    }

    /// Keep the text a document notification carries, under its settled path.
    fn remember(&self, uri: &Uri, source: &str) -> Option<PathBuf> {
        let path = settle(&uri.to_file_path()?);
        self.documents
            .lock()
            .expect("the document map is not poisoned")
            .insert(path.clone(), source.to_owned());
        Some(path)
    }

    /// Every open buffer over the disk, so the tree the editor sees is the
    /// tree an analysis reads.
    fn overlay(&self) -> OverlaySource {
        let documents = self
            .documents
            .lock()
            .expect("the document map is not poisoned");
        let mut overlay = OverlaySource::new();
        for (path, source) in documents.iter() {
            overlay.open(path.clone(), source.clone());
        }
        overlay
    }

    /// Analyze the tree the document belongs to and publish the facts its
    /// manifest asks for.
    async fn check(&self, uri: &Uri) {
        let Some(raw) = uri.to_file_path() else {
            return;
        };
        let path = settle(&raw);
        let Some(uri) = Uri::from_file_path(&path) else {
            return;
        };
        let context = match self.context(&path) {
            Ok(context) => context,
            Err(reason) => {
                self.inactive(&path, uri.clone(), reason).await;
                return;
            }
        };
        let tree = match &context {
            Context::Program(project) => Tree::Program(project),
            Context::Library(root) => Tree::StandardLibrary(root),
        };
        let scope = match &tree {
            Tree::Program(project) => project.scope,
            Tree::StandardLibrary(_) => DiagnosticScope::File,
        };
        let key = match &tree {
            Tree::Program(project) => project.manifest.clone(),
            Tree::StandardLibrary(root) => root.to_path_buf(),
        };
        let encoding = self.encoding.get().copied().unwrap_or(Encoding::Utf16);
        let overlay = self.overlay();
        let reports = analyze_tree(tree, &overlay);
        let source = self
            .documents
            .lock()
            .expect("the document map is not poisoned")
            .get(&path)
            .cloned()
            .or_else(|| std::fs::read_to_string(&path).ok())
            .unwrap_or_default();
        match scope {
            DiagnosticScope::File => {
                let current = reports.iter().find(|file| file.path == path);
                let diagnostics: Vec<ProtocolDiagnostic> = current
                    .map(|file| file.diagnostics.as_slice())
                    .unwrap_or(&[])
                    .iter()
                    .map(|diagnostic| protocol_diagnostic(&source, &path, diagnostic, encoding))
                    .collect();
                self.client
                    .publish_diagnostics(uri, diagnostics, None)
                    .await;
                let stale = self.remember_published(&key, std::slice::from_ref(&path));
                for path in stale {
                    self.clear(&path).await;
                }
            }
            DiagnosticScope::Tree => {
                let mut files: HashMap<PathBuf, (String, Vec<Diagnostic>)> = reports
                    .into_iter()
                    .map(|file| (file.path, (file.source, file.diagnostics)))
                    .collect();
                // The open file is published even when clean, so an earlier
                // publication is cleared.
                files
                    .entry(path.clone())
                    .or_insert_with(|| (source, Vec::new()));
                files.retain(|path, _| path.is_absolute());
                let current: Vec<PathBuf> = files.keys().cloned().collect();
                let stale = self.remember_published(&key, &current);
                for (path, (source, diagnostics)) in &files {
                    let Some(uri) = Uri::from_file_path(path) else {
                        continue;
                    };
                    let diagnostics = diagnostics
                        .iter()
                        .map(|diagnostic| protocol_diagnostic(source, path, diagnostic, encoding))
                        .collect();
                    self.client
                        .publish_diagnostics(uri, diagnostics, None)
                        .await;
                }
                for path in stale {
                    self.clear(&path).await;
                }
            }
        }
    }

    /// Record what this context published, returning the files that dropped
    /// out of the new set.
    fn remember_published(&self, key: &Path, current: &[PathBuf]) -> Vec<PathBuf> {
        let mut published = self
            .published
            .lock()
            .expect("the publication map is not poisoned");
        let previous = published.entry(key.to_path_buf()).or_default();
        let current: HashSet<PathBuf> = current.iter().cloned().collect();
        let stale: Vec<PathBuf> = previous.difference(&current).cloned().collect();
        *previous = current;
        stale
    }

    /// Clear one file's diagnostics.
    async fn clear(&self, path: &Path) {
        if let Some(uri) = Uri::from_file_path(path) {
            self.client.publish_diagnostics(uri, Vec::new(), None).await;
        }
    }

    /// Clear the open file and explain once why it is not analyzed.
    async fn inactive(&self, path: &Path, uri: Uri, reason: String) {
        self.client.publish_diagnostics(uri, Vec::new(), None).await;
        let first = self
            .silenced
            .lock()
            .expect("the message set is not poisoned")
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
            self.documents
                .lock()
                .expect("the document map is not poisoned")
                .remove(&settle(&path));
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
        let context = match self.context(&path) {
            Ok(context) => context,
            Err(_) => return Ok(None),
        };
        let tree = match &context {
            Context::Program(project) => Tree::Program(project),
            Context::Library(root) => Tree::StandardLibrary(root),
        };
        let source = self
            .documents
            .lock()
            .expect("the document map is not poisoned")
            .get(&path)
            .cloned()
            .or_else(|| std::fs::read_to_string(&path).ok());
        let Some(source) = source else {
            return Ok(None);
        };
        let encoding = *self.encoding.get().unwrap_or(&Encoding::Utf16);
        let overlay = self.overlay();
        let tokens = semantic_tokens(&path, &overlay, tree);
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

/// The library's source tree above a file: an ancestor named `std` that holds
/// every file the binary embeds. The check reads the tree's contents, so a
/// checkout at any path is recognized by an installed binary too.
fn standard_library_root(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|ancestor| {
            ancestor.file_name().is_some_and(|name| name == STD_TREE)
                && EMBEDDED
                    .iter()
                    .all(|(relative, _)| ancestor.join(relative).is_file())
        })
        .map(Path::to_path_buf)
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

    #[test]
    fn the_library_tree_is_recognized_by_its_contents() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("std/dynamic/map.kiru");
        let root = standard_library_root(&path).expect("the checkout's std tree is the library");
        assert!(root.ends_with("std"));
        assert_eq!(
            root,
            path.ancestors()
                .nth(2)
                .expect("the library root")
                .canonicalize()
                .unwrap()
        );
        let project = tempfile::tempdir().expect("a temporary directory");
        std::fs::create_dir_all(project.path().join("std")).expect("create the directory");
        std::fs::write(project.path().join("std/main.kiru"), "fn main() {};").expect("write");
        assert!(
            standard_library_root(&project.path().join("std/main.kiru")).is_none(),
            "a directory named std without the library's files is a program namespace"
        );
    }

    #[test]
    fn the_whole_library_analyzes_without_errors() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("std");
        let root = root.canonicalize().expect("the library is readable");
        let facts = analyze_tree(Tree::StandardLibrary(&root), &crate::cli::DiskSource);
        let errors: Vec<_> = facts
            .iter()
            .flat_map(|file| &file.diagnostics)
            .filter(|diagnostic| matches!(diagnostic.severity, Severity::Error))
            .collect();
        assert!(errors.is_empty(), "the library checks clean: {errors:?}");
        assert!(
            facts
                .iter()
                .flat_map(|file| &file.diagnostics)
                .all(|diagnostic| !diagnostic.message.contains("must end with")),
            "no body reports a false fall-through"
        );
    }
}
