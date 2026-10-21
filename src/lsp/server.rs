//! The language server's session: the handshake, the overlay, and the loop.
//!
//! The server keeps one overlay of editor buffers and rebuilds the whole
//! program through the compiler's build facade on every change. A rebuild is
//! synchronous and takes the whole program, so sequential request handling is
//! the loop: a change rebuilds once and the diagnostics publish once. A
//! compile failure is diagnostics, not a session failure, and a malformed
//! request answers an error, not a crash.

use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};

use lsp_server::{Connection, ErrorCode, Message, Request, Response};
use lsp_types::request::{GotoDefinition, HoverRequest, Request as _, Shutdown};
use lsp_types::{
    Diagnostic as ProtocolDiagnostic, DiagnosticRelatedInformation, DiagnosticSeverity,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, GotoDefinitionParams, Hover, HoverContents, InitializeParams,
    Location, MarkedString, OneOf, PositionEncodingKind, PublishDiagnosticsParams,
    ServerCapabilities, TextDocumentPositionParams, TextDocumentSyncCapability,
    TextDocumentSyncKind, Url,
};

use crate::compiler::{Build, Diagnostic, FileDiagnostics, OverlaySource, Severity, build};
use crate::lsp::positions::{Encoding, position_to_offset, span_to_range};

/// The name the language server reports under.
const SERVER_NAME: &str = "kls";

/// The entry file the server compiles when the command line names none.
const DEFAULT_ENTRY_NAME: &str = "main.kiru";

/// The state one session holds: the editor overlay, the entry the program
/// compiles from, the position encoding the two sides negotiated, the last
/// build, and the sources that build read.
struct Session {
    overlay: OverlaySource,
    entry: PathBuf,
    encoding: Encoding,
    build: Option<Build>,
    sources: HashMap<PathBuf, String>,
}

/// Run the language server over the process's standard streams. The one
/// argument, when given, names the entry file; without it the entry is
/// `main.kiru` under the workspace root the client names. Returns the process
/// exit code.
pub fn run() -> i32 {
    let mut words = std::env::args().skip(1);
    let entry = match (words.next(), words.next()) {
        (None, None) => None,
        (Some(entry), None) => Some(PathBuf::from(entry)),
        _ => {
            eprintln!("{SERVER_NAME}: the arguments name one entry file at most");
            return 2;
        }
    };
    let (connection, io_threads) = Connection::stdio();
    let code = match serve(connection, entry) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("{SERVER_NAME}: {error}");
            1
        }
    };
    if let Err(error) = io_threads.join() {
        eprintln!("{SERVER_NAME}: the streams ended on an error: {error}");
    }
    code
}

/// Serve one session over `connection`, compiling the program from the entry
/// the caller names. Returns when the client sends `exit` or disconnects.
pub fn serve(
    connection: Connection,
    entry: Option<PathBuf>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let (id, raw) = connection.initialize_start()?;
    let root = workspace_entry(&raw);
    let params: InitializeParams = serde_json::from_value(raw).unwrap_or_default();
    let entry = entry.or(root).unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_default()
            .join(DEFAULT_ENTRY_NAME)
    });
    let capabilities = ServerCapabilities {
        position_encoding: Some(encoding_kind(&params)),
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        hover_provider: Some(lsp_types::HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        ..ServerCapabilities::default()
    };
    let result = serde_json::json!({
        "capabilities": capabilities,
        "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
    });
    connection.initialize_finish(id, result)?;

    let mut session = Session {
        overlay: OverlaySource::new(),
        entry,
        encoding: negotiated(&params),
        build: None,
        sources: HashMap::new(),
    };
    rebuild_and_publish(&connection, &mut session)?;

    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                if request.method == Shutdown::METHOD {
                    connection
                        .sender
                        .send(Response::new_ok(request.id, ()).into())?;
                    continue;
                }
                let response = respond(&session, request);
                connection.sender.send(response.into())?;
            }
            Message::Notification(notification) => {
                if notification.method == "exit" {
                    break;
                }
                let changes_document = notification.method.starts_with("textDocument/did");
                observe(&mut session, notification);
                if changes_document {
                    rebuild_and_publish(&connection, &mut session)?;
                }
            }
            // The server sends no requests, so no response ever arrives.
            Message::Response(_) => {}
        }
    }
    Ok(())
}

/// Answer one request. A request the server does not serve answers
/// `MethodNotFound`; one whose parameters do not parse answers
/// `InvalidParams`. Neither ends the session.
fn respond(session: &Session, request: Request) -> Response {
    let invalid_params = |request_id: lsp_server::RequestId, method: &str| {
        Response::new_err(
            request_id,
            ErrorCode::InvalidParams as i32,
            format!("the parameters of {method} do not parse"),
        )
    };
    let request_id = request.id.clone();
    let method = request.method.clone();
    match method.as_str() {
        GotoDefinition::METHOD => match request.extract::<GotoDefinitionParams>(&method) {
            Ok((_, params)) => {
                let result = position_of(session, &params.text_document_position_params)
                    .and_then(|(path, offset)| {
                        session
                            .build
                            .as_ref()
                            .and_then(|built| built.definition(&path, offset))
                    })
                    .and_then(|(target, span)| {
                        Some(Location {
                            uri: file_uri(&target)?,
                            range: span_to_range(
                                &session.source_of(&target),
                                span,
                                session.encoding,
                            ),
                        })
                    });
                Response::new_ok(request_id, result)
            }
            Err(_) => invalid_params(request_id, &method),
        },
        HoverRequest::METHOD => match request.extract::<TextDocumentPositionParams>(&method) {
            Ok((_, at)) => {
                let text = position_of(session, &at).and_then(|(path, offset)| {
                    session
                        .build
                        .as_ref()
                        .and_then(|built| built.hover(&path, offset))
                });
                let result = text.map(|value| Hover {
                    contents: HoverContents::Scalar(MarkedString::String(value)),
                    range: None,
                });
                Response::new_ok(request_id, result)
            }
            Err(_) => invalid_params(request_id, &method),
        },
        _ => Response::new_err(
            request_id,
            ErrorCode::MethodNotFound as i32,
            format!("{method} is not served"),
        ),
    }
}

/// The path and the byte offset a position names, read against the source of
/// the last build. A file the last build did not read has no source to read
/// the position against, so it names the empty text.
fn position_of(session: &Session, at: &TextDocumentPositionParams) -> Option<(PathBuf, usize)> {
    let path = file_path(&at.text_document.uri)?;
    let source = session.source_of(&path);
    let offset = position_to_offset(&source, &at.position, session.encoding);
    Some((path, offset))
}

/// Observe one notification. The four text-document notifications maintain
/// the overlay; the rest change nothing. A notification whose parameters do
/// not parse is reported on the log stream and the session goes on.
fn observe(session: &mut Session, notification: lsp_server::Notification) {
    match notification.method.as_str() {
        "textDocument/didOpen" => {
            if let Ok(params) =
                serde_json::from_value::<DidOpenTextDocumentParams>(notification.params)
            {
                let path = file_path(&params.text_document.uri);
                if let Some(path) = path {
                    session.overlay.open(path, params.text_document.text);
                }
            }
        }
        "textDocument/didChange" => {
            if let Ok(params) =
                serde_json::from_value::<DidChangeTextDocumentParams>(notification.params)
            {
                // The sync is full: the last change carries the whole text.
                if let Some(change) = params.content_changes.last()
                    && let Some(path) = file_path(&params.text_document.uri)
                {
                    session.overlay.change(path, change.text.clone());
                }
            }
        }
        "textDocument/didSave" => {
            if let Ok(params) =
                serde_json::from_value::<DidSaveTextDocumentParams>(notification.params)
                && let Some(text) = params.text
                && let Some(path) = file_path(&params.text_document.uri)
            {
                session.overlay.change(path, text);
            }
        }
        "textDocument/didClose" => {
            if let Ok(params) =
                serde_json::from_value::<DidCloseTextDocumentParams>(notification.params)
                && let Some(path) = file_path(&params.text_document.uri)
            {
                session.overlay.close(&path);
            }
        }
        // The rest change nothing: the session serves no other feature yet.
        _ => {}
    }
}

/// Rebuild the whole program through the facade and publish every file's
/// diagnostics. A build that failed still publishes what it named.
fn rebuild_and_publish(
    connection: &Connection,
    session: &mut Session,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let files = match build(&session.entry, &session.overlay) {
        Ok(built) => {
            for (path, source) in built.file_sources() {
                session.sources.insert(path, source);
            }
            let files = built.diagnostics();
            session.build = Some(built);
            files
        }
        Err(failure) => {
            for (path, source) in failure.file_sources() {
                session.sources.insert(path, source);
            }
            let files = failure.diagnostics();
            session.build = None;
            files
        }
    };
    for file in &files {
        publish(connection, session, file)?;
    }
    Ok(())
}

/// Publish one file's diagnostics under the file's URI.
fn publish(
    connection: &Connection,
    session: &Session,
    file: &FileDiagnostics,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let Some(uri) = file_uri(&file.path) else {
        return Ok(());
    };
    let diagnostics = file
        .diagnostics
        .iter()
        .map(|diagnostic| protocol_diagnostic(session, &file.source, diagnostic))
        .collect();
    connection.sender.send(
        lsp_server::Notification::new(
            "textDocument/publishDiagnostics".to_owned(),
            PublishDiagnosticsParams {
                uri,
                diagnostics,
                version: None,
            },
        )
        .into(),
    )?;
    Ok(())
}

/// The compiler's diagnostic, as the protocol carries it: the severity and
/// the range from the negotiated encoding, and the related places as
/// locations.
fn protocol_diagnostic(
    session: &Session,
    source: &str,
    diagnostic: &Diagnostic,
) -> ProtocolDiagnostic {
    let severity = match diagnostic.severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
    };
    let related_information: Vec<DiagnosticRelatedInformation> = diagnostic
        .related
        .iter()
        .filter_map(|related| {
            let uri = file_uri(&related.path)?;
            let related_source = session.source_of(&related.path);
            Some(DiagnosticRelatedInformation {
                location: Location {
                    uri,
                    range: span_to_range(&related_source, related.span, session.encoding),
                },
                message: related.message.clone(),
            })
        })
        .collect();
    ProtocolDiagnostic {
        range: span_to_range(source, diagnostic.span, session.encoding),
        severity: Some(severity),
        code: None,
        code_description: None,
        source: Some(SERVER_NAME.to_owned()),
        message: diagnostic.message.clone(),
        related_information: Some(related_information).filter(|related| !related.is_empty()),
        tags: None,
        data: None,
    }
}

/// The encoding the two sides negotiate: UTF-8 when the client offers it,
/// UTF-16 otherwise, as the spec requires a server to answer.
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

fn encoding_kind(params: &InitializeParams) -> PositionEncodingKind {
    match negotiated(params) {
        Encoding::Utf8 => PositionEncodingKind::UTF8,
        Encoding::Utf16 => PositionEncodingKind::UTF16,
    }
}

/// The entry the workspace root names, when the client named one. The
/// workspace folders are the current form of the root; the deprecated
/// `rootUri` answers for the clients that still send only it.
fn workspace_entry(raw: &serde_json::Value) -> Option<PathBuf> {
    let root = raw
        .get("workspaceFolders")
        .and_then(|folders| folders.as_array())
        .and_then(|folders| folders.first())
        .and_then(|folder| folder.get("uri"))
        .and_then(|uri| uri.as_str())
        .map(|spelled| spelled.to_owned())
        .or_else(|| {
            raw.get("rootUri")?
                .as_str()
                .map(|spelled| spelled.to_owned())
        })?;
    let root = Url::parse(&root).ok()?.to_file_path().ok()?;
    Some(root.join(DEFAULT_ENTRY_NAME))
}

/// The file path a document URI names.
fn file_path(uri: &Url) -> Option<PathBuf> {
    uri.to_file_path().ok()
}

/// The URI a file path names.
fn file_uri(path: &Path) -> Option<Url> {
    Url::from_file_path(path).ok()
}

impl Session {
    /// The text of one file as the last build read it.
    fn source_of(&self, path: &Path) -> String {
        self.sources.get(path).cloned().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::Span;
    use lsp_server::ResponseKind;
    use lsp_types::Position;
    use lsp_types::{
        DidChangeTextDocumentParams, InitializeResult, TextDocumentContentChangeEvent,
        TextDocumentItem, VersionedTextDocumentIdentifier,
    };

    /// The client side of a session: it speaks raw LSP messages over the
    /// channels a memory connection pairs.
    struct Client {
        connection: Connection,
    }

    impl Client {
        fn request(&self, id: i32, method: &str, params: serde_json::Value) {
            self.connection
                .sender
                .send(Message::Request(lsp_server::Request {
                    id: id.into(),
                    method: method.to_owned(),
                    params,
                }))
                .expect("the server is listening");
        }

        fn notify(&self, method: &str, params: serde_json::Value) {
            self.connection
                .sender
                .send(Message::Notification(lsp_server::Notification {
                    method: method.to_owned(),
                    params,
                }))
                .expect("the server is listening");
        }

        /// Read until one response arrives, returning every notification
        /// passed on the way.
        fn response(&self, id: i32) -> (Response, Vec<lsp_server::Notification>) {
            let mut seen = Vec::new();
            loop {
                match self
                    .connection
                    .receiver
                    .recv()
                    .expect("the server is running")
                {
                    Message::Response(response) => {
                        assert_eq!(response.id, lsp_server::RequestId::from(id));
                        return (response, seen);
                    }
                    Message::Notification(notification) => seen.push(notification),
                    Message::Request(_) => {}
                }
            }
        }

        /// The session served: the thread's answer.
        fn served(server: std::thread::JoinHandle<bool>) {
            assert!(server.join().expect("the session served"));
        }
    }

    /// The program the session compiles, written to a directory on the disk.
    fn program(source: &str) -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("main.kiru");
        std::fs::write(&path, source).expect("write file");
        (directory, path)
    }

    /// The document URI the entry names.
    fn uri_of(path: &Path) -> Url {
        Url::from_file_path(path).expect("a temp path is a file path")
    }

    /// The position a byte offset stands at, in UTF-8 columns.
    fn utf8_position(text: &str, offset: usize) -> Position {
        let line = text[..offset].matches('\n').count() as u32;
        let line_start = text[..offset]
            .rfind('\n')
            .map(|index| index + 1)
            .unwrap_or(0);
        Position {
            line,
            character: (offset - line_start) as u32,
        }
    }

    /// The position a byte offset stands at, in UTF-16 columns.
    fn utf16_position(text: &str, offset: usize) -> Position {
        let line = text[..offset].matches('\n').count() as u32;
        let line_start = text[..offset]
            .rfind('\n')
            .map(|index| index + 1)
            .unwrap_or(0);
        Position {
            line,
            character: text[line_start..offset]
                .chars()
                .map(char::len_utf16)
                .sum::<usize>() as u32,
        }
    }

    /// The span of the first place `needle` stands in `text`.
    fn byte_span(text: &str, needle: &str) -> Span {
        let start = text.find(needle).expect("the needle is written");
        Span::new(start, start + needle.len())
    }

    /// The latest publication of one URI in the notifications.
    fn diagnostics_of(
        notifications: &[lsp_server::Notification],
        uri: &Url,
    ) -> Vec<ProtocolDiagnostic> {
        notifications
            .iter()
            .rev()
            .find(|notification| notification.method == "textDocument/publishDiagnostics")
            .map(|notification| {
                serde_json::from_value::<PublishDiagnosticsParams>(notification.params.clone())
                    .expect("the publication parses")
            })
            .filter(|published| &published.uri == uri)
            .map(|published| published.diagnostics)
            .unwrap_or_default()
    }

    /// The didOpen parameters a buffer opening names.
    fn did_open(uri: &Url, text: &str) -> serde_json::Value {
        serde_json::to_value(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "kiru".to_owned(),
                version: 1,
                text: text.to_owned(),
            },
        })
        .expect("the parameters serialize")
    }

    /// The didChange parameters a full-text change names.
    fn did_change(uri: &Url, text: &str) -> serde_json::Value {
        serde_json::to_value(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version: 2,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: text.to_owned(),
            }],
        })
        .expect("the parameters serialize")
    }

    /// The position parameters a query names.
    fn at(uri: &Url, position: Position) -> serde_json::Value {
        serde_json::json!({
            "textDocument": { "uri": uri },
            "position": position,
        })
    }

    /// The result of an answered request, when it answered with one.
    fn answered(response: &Response) -> serde_json::Value {
        let ResponseKind::Ok { result } = &response.response_kind else {
            panic!("the request answered a result");
        };
        result.clone()
    }

    #[test]
    fn a_session_publishes_diagnostics_and_answers_the_queries() {
        let on_disk =
            "fn speak() -> txt { return \"spoken\"; };\nfn main() { std::print(speak()); };";
        let (_directory, entry) = program(on_disk);
        let uri = uri_of(&entry);
        let (server_side, client_side) = Connection::memory();
        let server = std::thread::spawn(move || serve(server_side, Some(entry)).is_ok());
        let client = Client {
            connection: client_side,
        };

        // The handshake: the client offers both encodings; the server picks
        // UTF-8, the compiler's own.
        client.request(
            1,
            "initialize",
            serde_json::json!({
                "capabilities": { "general": { "positionEncodings": ["utf-16", "utf-8"] } },
            }),
        );
        let (response, _) = client.response(1);
        let result: InitializeResult =
            serde_json::from_value(answered(&response)).expect("the result parses");
        assert_eq!(
            result.capabilities.position_encoding,
            Some(PositionEncodingKind::UTF8),
            "the encoding negotiated is UTF-8"
        );
        client.notify("initialized", serde_json::json!({}));

        // The open buffer carries one error and one unused `use` over the
        // disk program. The publication names the error; a build that fails
        // names no warnings.
        let opened = "use root::speak;\nuse std::text;\nfn speak() -> txt { return \"spoken\"; };\nfn main() { nope(); };";
        client.notify("textDocument/didOpen", did_open(&uri, opened));
        client.request(2, "textDocument/hover", at(&uri, Position::default()));
        let (response, published) = client.response(2);
        assert!(answered(&response).is_null(), "nothing to hover on");
        let diagnostics = diagnostics_of(&published, &uri);
        assert_eq!(diagnostics.len(), 1, "the error is published");
        assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(diagnostics[0].message, "unknown name `nope`");
        let expected = byte_span(opened, "nope");
        assert_eq!(
            diagnostics[0].range,
            lsp_types::Range {
                start: utf8_position(opened, expected.start),
                end: utf8_position(opened, expected.end),
            }
        );

        // The buffer changes to a program that checks: the publication
        // clears the error and carries the two unused `use`s as warnings.
        let fixed = "use root::speak;\nuse std::text;\nfn speak() -> txt { return \"spoken\"; };\nfn main() { std::print(speak()); };";
        client.notify("textDocument/didChange", did_change(&uri, fixed));
        client.request(3, "textDocument/hover", at(&uri, Position::default()));
        let (_, published) = client.response(3);
        let diagnostics = diagnostics_of(&published, &uri);
        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.severity == Some(DiagnosticSeverity::WARNING))
                .count(),
            2,
            "the two unused `use`s are published: {diagnostics:?}"
        );
        assert!(
            !diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message == "unknown name `nope`"),
            "the error cleared"
        );
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message == "unused use `root::speak`")
                && diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.message == "unused use `std::text`")
        );

        // A position on the read of `speak` answers the declaration: the
        // same file, the written name's span.
        let read = byte_span(fixed, "speak());");
        client.request(
            4,
            "textDocument/definition",
            at(&uri, utf8_position(fixed, read.start)),
        );
        let (response, _) = client.response(4);
        let location: Location =
            serde_json::from_value(answered(&response)).expect("the definition answers a location");
        // The name span: `speak` is five bytes wide.
        let declared_name = byte_span(fixed, "speak() -> txt");
        let declared = Span::new(declared_name.start, declared_name.start + 5);
        assert_eq!(location.uri, uri);
        assert_eq!(
            location.range,
            lsp_types::Range {
                start: utf8_position(fixed, declared.start),
                end: utf8_position(fixed, declared.end),
            }
        );

        // The same position hovers: the declaration's name and the type the
        // check settled.
        client.request(
            5,
            "textDocument/hover",
            at(&uri, utf8_position(fixed, read.start)),
        );
        let (response, _) = client.response(5);
        let hover: Hover = serde_json::from_value(answered(&response)).expect("the hover parses");
        let HoverContents::Scalar(MarkedString::String(text)) = hover.contents else {
            panic!("the hover carries text");
        };
        assert_eq!(text, "root::speak: fn() -> txt");

        // A method the server does not serve answers MethodNotFound; one
        // whose parameters do not parse answers InvalidParams. The session
        // stays up for the next request either way.
        client.request(6, "textDocument/completion", serde_json::json!({}));
        let (response, _) = client.response(6);
        let ResponseKind::Err { error } = response.response_kind else {
            panic!("an unserved request answers an error");
        };
        assert_eq!(error.code, ErrorCode::MethodNotFound as i32);
        client.request(
            7,
            "textDocument/definition",
            serde_json::json!({"position": "no"}),
        );
        let (response, _) = client.response(7);
        let ResponseKind::Err { error } = response.response_kind else {
            panic!("an unparseable request answers an error");
        };
        assert_eq!(error.code, ErrorCode::InvalidParams as i32);

        // The session ends the protocol way: shutdown, then exit.
        client.request(8, "shutdown", serde_json::json!(null));
        let (response, _) = client.response(8);
        assert_eq!(answered(&response), serde_json::json!(null));
        client.notify("exit", serde_json::json!({}));
        Client::served(server);
    }

    #[test]
    fn a_session_without_the_utf8_offer_falls_back_to_utf16() {
        // The entry declares a read after a non-BMP character on one line, so
        // a UTF-16 column and a byte offset differ for it.
        let on_disk = "fn main() { let s: txt = \"🦀\"; std::print(s); };";
        let (_directory, entry) = program(on_disk);
        let uri = uri_of(&entry);
        let (server_side, client_side) = Connection::memory();
        let server = std::thread::spawn(move || serve(server_side, Some(entry)).is_ok());
        let client = Client {
            connection: client_side,
        };

        // No encodings are offered: the server answers UTF-16.
        client.request(1, "initialize", serde_json::json!({ "capabilities": {} }));
        let (response, _) = client.response(1);
        let result: InitializeResult =
            serde_json::from_value(answered(&response)).expect("the result parses");
        assert_eq!(
            result.capabilities.position_encoding,
            Some(PositionEncodingKind::UTF16),
            "the encoding negotiated is UTF-16"
        );
        client.notify("initialized", serde_json::json!({}));

        // A definition at the read of `s`, named in UTF-16 columns, answers
        // the binding's own name.
        let read = byte_span(on_disk, "s);");
        client.request(
            2,
            "textDocument/definition",
            at(&uri, utf16_position(on_disk, read.start)),
        );
        let (response, _) = client.response(2);
        let location: Location =
            serde_json::from_value(answered(&response)).expect("the definition answers a location");
        // The name span: `s` is one byte wide.
        let declared_name = byte_span(on_disk, "s: txt");
        let declared = Span::new(declared_name.start, declared_name.start + 1);
        assert_eq!(location.uri, uri);
        assert_eq!(
            location.range,
            lsp_types::Range {
                start: utf16_position(on_disk, declared.start),
                end: utf16_position(on_disk, declared.end),
            }
        );

        client.request(3, "shutdown", serde_json::json!(null));
        let _ = client.response(3);
        client.notify("exit", serde_json::json!({}));
        Client::served(server);
    }
}
