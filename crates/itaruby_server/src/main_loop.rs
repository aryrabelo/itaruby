//! The synchronous LSP message loop: one thread dispatches protocol messages,
//! one worker thread recomputes diagnostics. Cancellation, not a timer, is
//! the debounce: a new `set_text` on the main thread's `Db` clone cancels
//! whatever the worker is computing on its own clone, and the worker starts
//! over from the freshest snapshot.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Write as _;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::thread;

use anyhow::Result;
use crossbeam_channel::Sender;
use lsp_server::{Connection, ErrorCode, IoThreads, Message, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument,
    LogMessage, Notification as NotificationTrait, PublishDiagnostics,
};
use lsp_types::request::{
    DocumentDiagnosticRequest, GotoDefinition, HoverRequest, Request as RequestTrait,
};
use lsp_types::{
    Diagnostic as LspDiagnostic, DiagnosticOptions, DiagnosticServerCapabilities,
    DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DocumentDiagnosticParams, DocumentDiagnosticReport,
    DocumentDiagnosticReportResult, FullDocumentDiagnosticReport, GotoDefinitionParams,
    GotoDefinitionResponse, Hover, HoverContents, HoverParams, HoverProviderCapability,
    InitializeParams, InitializeResult, Location, LogMessageParams, MarkupContent, MarkupKind,
    MessageType, NumberOrString, OneOf, Position, PublishDiagnosticsParams, Range,
    RelatedFullDocumentDiagnosticReport, ServerCapabilities, ServerInfo,
    TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
};
use itaruby_semantic::{
    check_file, definition_at, hover_at, wire_declaration_sources, Db, Diagnostic as SemDiagnostic,
    DiscoveredSources, LineIndex, ProjectFiles, Severity, SourceFile,
};
use salsa::Setter as _;

/// Runs the server to completion: initialize handshake, message loop,
/// shutdown handshake, then joins the stdio threads.
///
/// `connection` (and its `Sender<Message>`) MUST be fully dropped before
/// `io_threads.join()`: the writer thread only exits once every sender
/// clone is gone, so `message_loop` takes `connection` by value and drops
/// it on return, and only then do we join.
///
/// # Errors
///
/// Returns `Err` if the LSP initialize handshake or message loop fails
/// (malformed params, a broken stdio channel), or if the stdio I/O threads
/// fail to join cleanly.
pub fn run(conn: (Connection, IoThreads)) -> Result<()> {
    let (connection, io_threads) = conn;
    message_loop(connection)?;
    io_threads.join()?;
    Ok(())
}

// `connection` must be owned here (not `&Connection`): the doc comment on
// `run` above depends on this function dropping every `Sender<Message>`
// clone it holds — including `diagnostics_sender`, moved into the worker
// thread below — before returning, so `run`'s `io_threads.join()` never
// races the writer thread's own shutdown.
#[expect(
    clippy::needless_pass_by_value,
    reason = "connection must be owned so it (and its Sender clones) fully drops on return, before run()'s io_threads.join() — see run's doc comment"
)]
fn message_loop(connection: Connection) -> Result<()> {
    let (initialize_id, initialize_params) = connection.initialize_start()?;
    let params: InitializeParams = serde_json::from_value(initialize_params)?;

    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        definition_provider: Some(OneOf::Left(true)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        diagnostic_provider: Some(DiagnosticServerCapabilities::Options(DiagnosticOptions {
            identifier: Some("itaruby".to_string()),
            inter_file_dependencies: true,
            workspace_diagnostics: false,
            ..Default::default()
        })),
        ..Default::default()
    };
    let result = InitializeResult {
        capabilities,
        server_info: Some(ServerInfo {
            name: "itaruby".to_string(),
            version: None,
        }),
    };
    connection.initialize_finish(initialize_id, serde_json::to_value(result)?)?;

    let root = workspace_root(&params);
    let project_root = ProjectRoot::new(&root);
    let mut warned_out_of_root: HashSet<PathBuf> = HashSet::new();

    let (mut db, mut files, declarations_only) = load_workspace(&connection, &root)?;

    let mut open: HashMap<PathBuf, i32> = HashMap::new();

    let (work_tx, work_rx) = crossbeam_channel::unbounded::<RecomputeRequest>();
    let diagnostics_sender = connection.sender.clone();
    let worker = thread::spawn(move || diagnostics_worker(&work_rx, &diagnostics_sender));

    // Request queue + cancellation set. A request is DISPATCHED only after
    // every notification already sitting in the channel has been absorbed:
    // didChange first (so the salsa snapshot is the freshest revision and
    // no position request ever answers about text the client has already
    // replaced), then the cancel set — a request cancelled before dispatch
    // answers RequestCancelled, per the LSP spec.
    //
    // ponytail: single-threaded dispatch, no worker-per-request pool —
    // hover/definition are one salsa query each, and racing pools would
    // only fight over the same snapshot the diagnostics worker already
    // refreshes. Upgrade trigger: position requests measurably slower
    // than ~100ms on corpus-sized files — then dispatch on a pool and
    // cancel cooperatively via salsa revisions (`Cancelled::catch` at the
    // handlers already returns `null` safely, so that path is wired).
    let mut pending: VecDeque<Request> = VecDeque::new();
    let mut cancelled: HashSet<RequestId> = HashSet::new();

    loop {
        let req = if let Some(req) = pending.pop_front() {
            // Drain-before-respond: absorb everything that arrived while
            // this request was queued (and re-check the cancel set after).
            while let Ok(msg) = connection.receiver.try_recv() {
                absorb_message(
                    msg,
                    &mut db,
                    &mut files,
                    &mut open,
                    &connection,
                    &work_tx,
                    &mut pending,
                    &mut cancelled,
                    &project_root,
                    &mut warned_out_of_root,
                    &declarations_only,
                )?;
            }
            req
        } else {
            match connection.receiver.recv() {
                Ok(Message::Request(req)) => {
                    pending.push_back(req);
                    continue; // dispatch on the next iteration
                }
                Ok(msg) => {
                    absorb_message(
                        msg,
                        &mut db,
                        &mut files,
                        &mut open,
                        &connection,
                        &work_tx,
                        &mut pending,
                        &mut cancelled,
                        &project_root,
                        &mut warned_out_of_root,
                        &declarations_only,
                    )?;
                    continue;
                }
                Err(_) => break, // client went away
            }
        };

        if dispatch_request(&connection, req, &db, &files, &mut cancelled, &declarations_only)? {
            break;
        }
    }

    drop(work_tx);
    let _ = worker.join();
    Ok(())
    // `connection` (and its `Sender<Message>` clone held by `connection.sender`)
    // drops here, on return, before the caller joins the stdio threads.
}

/// Load the workspace into a fresh salsa db: walk `root` for `.rb`
/// sources, run the shared declaration-source discovery (bead ita-16g —
/// the same `itaruby_semantic::wire_declaration_sources` search `ita
/// check` runs: db/schema.rb > db/structure.sql, plus a client's
/// sorbet/rbi, searched upward from the workspace root), pull any
/// discovered declaration file into the project, and announce what was
/// found via `window/logMessage`.
///
/// Returns the db, the path→file map, and the declaration-only set —
/// files that are DECLARATION SOURCES, never code under review:
/// `schedule_recompute` skips them so a declaration file opened directly
/// by the client never gets a `publishDiagnostics`.
///
/// `ClosedWorld` is deliberately never wired here — only `ita check`
/// proves every checked root gem-safe first (see `ClosedWorld`'s doc
/// comment in `itaruby_semantic`); the server keeps v0's exact silent
/// behavior for core-class monkeypatches.
fn load_workspace(
    connection: &Connection,
    root: &Path,
) -> Result<(Db, HashMap<PathBuf, SourceFile>, HashSet<PathBuf>)> {
    let mut db = Db::new();
    let mut files: HashMap<PathBuf, SourceFile> = HashMap::new();
    for path in discover_rb_files(root) {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let file = SourceFile::new(&db, path.clone(), text);
        files.insert(path, file);
    }

    let discovered = wire_declaration_sources(&mut db, &[root.to_path_buf()]);
    let declarations_only: HashSet<PathBuf> =
        discovered.declarations_only.iter().cloned().collect();
    for schema in &discovered.declarations_only {
        if !files.contains_key(schema) {
            if let Ok(text) = std::fs::read_to_string(schema) {
                let file = SourceFile::new(&db, schema.clone(), text);
                files.insert(schema.clone(), file);
            }
        }
    }
    send_discovery_log(connection, &discovered)?;

    ProjectFiles::new(&db, files.values().copied().collect());

    Ok((db, files, declarations_only))
}

/// Answer one request, or its cancellation, and send the response.
/// Returns `true` when the request was `shutdown` — the caller then breaks
/// the message loop (the client's `exit` notification follows).
fn dispatch_request(
    connection: &Connection,
    req: Request,
    db: &Db,
    files: &HashMap<PathBuf, SourceFile>,
    cancelled: &mut HashSet<RequestId>,
    declarations_only: &HashSet<PathBuf>,
) -> Result<bool> {
    if connection.handle_shutdown(&req)? {
        return Ok(true);
    }
    if cancelled.remove(&req.id) {
        let resp = Response::new_err(
            req.id,
            ErrorCode::RequestCanceled as i32,
            "request cancelled".to_string(),
        );
        connection.sender.send(Message::Response(resp))?;
        return Ok(false);
    }
    let resp = match req.extract::<GotoDefinitionParams>(<GotoDefinition as RequestTrait>::METHOD) {
        Ok((id, params)) => handle_definition_request(id, &params, db, files),
        Err(lsp_server::ExtractError::MethodMismatch(req)) => {
            route_non_definition_request(req, db, files, declarations_only)?
        }
        Err(lsp_server::ExtractError::JsonError { method, error }) => {
            return Err(anyhow::anyhow!("malformed params for {method}: {error}"));
        }
    };
    connection.sender.send(Message::Response(resp))?;
    Ok(false)
}

/// The rest of `dispatch_request`'s extract chain, split out to stay under
/// the complexity ceiling: hover, then the `textDocument/diagnostic` pull
/// request (w4/bead ita-0fh), then method-not-found.
fn route_non_definition_request(
    req: Request,
    db: &Db,
    files: &HashMap<PathBuf, SourceFile>,
    declarations_only: &HashSet<PathBuf>,
) -> Result<Response> {
    match req.extract::<HoverParams>(<HoverRequest as RequestTrait>::METHOD) {
        Ok((id, params)) => Ok(handle_hover_request(id, &params, db, files)),
        Err(lsp_server::ExtractError::MethodMismatch(req)) => match req
            .extract::<DocumentDiagnosticParams>(<DocumentDiagnosticRequest as RequestTrait>::METHOD)
        {
            Ok((id, params)) => {
                Ok(handle_diagnostic_request(id, &params, db, files, declarations_only))
            }
            Err(lsp_server::ExtractError::MethodMismatch(req)) => {
                Ok(respond_method_not_found_value(req))
            }
            Err(lsp_server::ExtractError::JsonError { method, error }) => {
                Err(anyhow::anyhow!("malformed params for {method}: {error}"))
            }
        },
        Err(lsp_server::ExtractError::JsonError { method, error }) => {
            Err(anyhow::anyhow!("malformed params for {method}: {error}"))
        }
    }
}


/// Route one incoming message: requests go to the queue (dispatched in
/// order once the channel has been drained of notifications), `$/
/// cancelRequest` marks its id, every other notification mutates the
/// snapshot right away.
#[allow(clippy::too_many_arguments)] // reason: loop state stays flat (10 refs) so each arm shows exactly which fields it touches
fn absorb_message(
    msg: Message,
    db: &mut Db,
    files: &mut HashMap<PathBuf, SourceFile>,
    open: &mut HashMap<PathBuf, i32>,
    connection: &Connection,
    work_tx: &Sender<RecomputeRequest>,
    pending: &mut VecDeque<Request>,
    cancelled: &mut HashSet<RequestId>,
    root: &ProjectRoot,
    warned: &mut HashSet<PathBuf>,
    declarations_only: &HashSet<PathBuf>,
) -> Result<()> {
    match msg {
        Message::Request(req) => pending.push_back(req),
        Message::Notification(note) => {
            if note.method == "$/cancelRequest" {
                if let Some(id) = parse_cancel_id(&note.params) {
                    // Best-effort by spec: if the request already went
                    // out, the cancel is simply never consulted again.
                    cancelled.insert(id);
                }
                return Ok(());
            }
            let mut ctx = NotificationCtx { db, files, open, connection, work_tx, root, warned, declarations_only };
            handle_notification(note, &mut ctx)?;
        }
        Message::Response(_) => {
            // The server never issues requests of its own.
        }
    }
    Ok(())
}

/// `$/cancelRequest` params carry the request id as either a number or a
/// string — the two shapes `RequestId` itself allows.
fn parse_cancel_id(params: &serde_json::Value) -> Option<RequestId> {
    match params.get("id")? {
        serde_json::Value::Number(n) => n
            .as_i64()
            .and_then(|v| i32::try_from(v).ok())
            .map(RequestId::from),
        serde_json::Value::String(s) => Some(RequestId::from(s.clone())),
        _ => None,
    }
}

/// Build (but do not send) the method-not-found response, so dispatch
/// stays one uniform send path.
fn respond_method_not_found_value(req: Request) -> Response {
    Response::new_err(
        req.id,
        ErrorCode::MethodNotFound as i32,
        format!("unhandled request: {}", req.method),
    )
}

/// `textDocument/definition`: same salsa-cancellation pattern as the
/// diagnostics worker (§ `diagnostics_worker`) — a query cancelled by a
/// concurrent edit answers `null`, same as "no definition found". Unknown
/// file, out-of-range position, or `definition_at` finding nothing all
/// converge on the same `null` response: never guess.
fn handle_definition_request(
    id: lsp_server::RequestId,
    params: &GotoDefinitionParams,
    db: &Db,
    files: &HashMap<PathBuf, SourceFile>,
) -> Response {
    let null = || Response::new_ok(id.clone(), Option::<GotoDefinitionResponse>::None);
    let doc = &params.text_document_position_params.text_document;
    let Some(path) = uri_to_path(&doc.uri) else {
        return null();
    };
    let Some(&file) = files.get(&path) else {
        return null();
    };
    let text = file.text(db);
    let index = LineIndex::new(text);
    let pos = params.text_document_position_params.position;
    let offset = index.offset(text, pos.line, pos.character);

    let site = salsa::Cancelled::catch(AssertUnwindSafe(|| definition_at(db, file, offset))).unwrap_or_default();
    let Some(site) = site else {
        return null();
    };
    let Ok(uri) = path_to_uri(site.file.path(db)) else {
        return null();
    };
    let site_text = site.file.text(db);
    let site_index = LineIndex::new(site_text);
    let (start_line, start_col) = site_index.line_col(site_text, site.start);
    let (end_line, end_col) = site_index.line_col(site_text, site.end);
    let location = Location {
        uri,
        range: Range {
            start: Position::new(start_line, start_col),
            end: Position::new(end_line, end_col),
        },
    };
    Response::new_ok(id, Some(GotoDefinitionResponse::Scalar(location)))
}

/// `textDocument/hover` (w4): the type of the innermost expression at the
/// position, and — when the cursor sits on a resolved call's method name —
/// that call's signature. Every "don't know" path (unknown file,
/// out-of-range position, `Ty::Unknown`, no resolved call, a query
/// cancelled by a concurrent edit) converges on the same `null` response:
/// hover is under invariant #1 too, it never invents a type.
fn handle_hover_request(
    id: lsp_server::RequestId,
    params: &HoverParams,
    db: &Db,
    files: &HashMap<PathBuf, SourceFile>,
) -> Response {
    let null = || Response::new_ok(id.clone(), Option::<Hover>::None);
    let doc = &params.text_document_position_params.text_document;
    let Some(path) = uri_to_path(&doc.uri) else {
        return null();
    };
    let Some(&file) = files.get(&path) else {
        return null();
    };
    let text = file.text(db);
    let index = LineIndex::new(text);
    let pos = params.text_document_position_params.position;
    let offset = index.offset(text, pos.line, pos.character);

    let info = salsa::Cancelled::catch(AssertUnwindSafe(|| hover_at(db, file, offset))).unwrap_or_default();
    let Some(info) = info else {
        return null();
    };

    let mut sections: Vec<String> = Vec::new();
    if let Some(ty) = &info.ty {
        sections.push(format!("```ruby\n{ty}\n```"));
    }
    if let Some(m) = &info.method {
        let site_text = m.site.file.text(db);
        let (line, _col) = LineIndex::new(site_text).line_col(site_text, m.site.start);
        sections.push(format!(
            "`{}` — arity {} — defined at {}:{}",
            m.name,
            m.arity,
            m.site.file.path(db).display(),
            line + 1
        ));
    }
    let hover = Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: sections.join("\n\n"),
        }),
        range: None,
    };
    Response::new_ok(id, Some(hover))
}

/// `textDocument/diagnostic` (bead ita-0fh, C1): the same diagnostics
/// `publishDiagnostics` would push for this file, pulled synchronously on
/// request — always a full report, never `unchanged`. `// ponytail:` no
/// `resultId`/unchanged caching; upgrade when pull traffic on a large
/// project measurably justifies it. An unknown path, a path never opened
/// on this server, or a declaration-only source (bead ita-16g — never
/// code under review) all converge on an empty report: pull is under
/// invariant #1 too, it never invents diagnostics for a file it can't
/// check.
fn handle_diagnostic_request(
    id: lsp_server::RequestId,
    params: &DocumentDiagnosticParams,
    db: &Db,
    files: &HashMap<PathBuf, SourceFile>,
    declarations_only: &HashSet<PathBuf>,
) -> Response {
    let empty_report = || {
        DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(
            RelatedFullDocumentDiagnosticReport {
                related_documents: None,
                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                    result_id: None,
                    items: Vec::new(),
                },
            },
        ))
    };
    let Some(path) = uri_to_path(&params.text_document.uri) else {
        return Response::new_ok(id, empty_report());
    };
    if declarations_only.contains(&path) {
        return Response::new_ok(id, empty_report());
    }
    let Some(&file) = files.get(&path) else {
        return Response::new_ok(id, empty_report());
    };
    let items = file_lsp_diagnostics(db, file).unwrap_or_default();
    let report = DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(
        RelatedFullDocumentDiagnosticReport {
            related_documents: None,
            full_document_diagnostic_report: FullDocumentDiagnosticReport { result_id: None, items },
        },
    ));
    Response::new_ok(id, report)
}

/// The server's workspace root, prepared once at startup for cheap
/// per-notification containment checks (contract #3): a path outside the
/// root is ignored whole, never merged into `ProjectFiles`.
struct ProjectRoot {
    original: PathBuf,
    canonical: PathBuf,
    normalized: PathBuf,
}

impl ProjectRoot {
    fn new(root: &Path) -> Self {
        let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let normalized = normalize_path(root);
        Self {
            original: root.to_path_buf(),
            canonical,
            normalized,
        }
    }

    /// True when `path` is the root itself or lives under it.
    /// `canonicalize` (which also resolves symlinks — e.g. macOS's `/tmp`
    /// pointing at `/private/tmp`) is the OS-verified answer whenever both
    /// sides exist on disk; a path that doesn't exist yet (client just
    /// created it, not flushed under that exact name) falls back to a
    /// purely lexical, component-wise `starts_with` — never a substring
    /// check, which would wrongly admit `/root-evil` under `/root`.
    fn contains(&self, path: &Path) -> bool {
        match path.canonicalize() {
            Ok(canonical) => canonical.starts_with(&self.canonical),
            Err(_) => normalize_path(path).starts_with(&self.normalized),
        }
    }
}

/// Resolves `.`/`..` path components purely by syntax — no filesystem
/// access, so it works for paths that don't exist yet. Only the fallback
/// for `ProjectRoot::contains`; `canonicalize` (which also follows
/// symlinks) is preferred whenever it succeeds.
fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// One `window/logMessage` (WARNING) telling the client a path it tried to
/// open sits outside the project root and will be ignored whole (contract
/// #3). The caller de-dupes via a `warned` set so repeatedly re-opening
/// the same out-of-root file doesn't spam this per keystroke.
fn send_out_of_root_warning(connection: &Connection, path: &Path, root: &Path) -> Result<()> {
    let message = format!(
        "itaruby: {} is outside the project root ({}); ignoring",
        path.display(),
        root.display()
    );
    let note = lsp_server::Notification::new(
        <LogMessage as NotificationTrait>::METHOD.to_string(),
        LogMessageParams {
            typ: MessageType::WARNING,
            message,
        },
    );
    connection.sender.send(Message::Notification(note))?;
    Ok(())
}

/// One `window/logMessage` (Info) per discovered declaration source (bead
/// ita-16g), or a single "no declaration sources found" message when
/// discovery found nothing — the server-side counterpart to
/// `send_out_of_root_warning`, so an editor's LSP output panel always
/// shows what `ita server` picked up on this workspace.
fn send_discovery_log(connection: &Connection, discovered: &DiscoveredSources) -> Result<()> {
    let mut lines = Vec::new();
    if let Some(schema) = &discovered.schema_rb {
        lines.push(format!("itaruby: discovered {} (schema.rb)", schema.display()));
    }
    if let Some(sql) = &discovered.structure_sql {
        lines.push(format!("itaruby: discovered {} (structure.sql)", sql.display()));
    }
    if let Some(rbi) = &discovered.rbi_dir {
        lines.push(format!("itaruby: discovered {} (sorbet/rbi)", rbi.display()));
    }
    if lines.is_empty() {
        lines.push("itaruby: no declaration sources found".to_string());
    }
    for message in lines {
        let note = lsp_server::Notification::new(
            <LogMessage as NotificationTrait>::METHOD.to_string(),
            LogMessageParams { typ: MessageType::INFO, message },
        );
        connection.sender.send(Message::Notification(note))?;
    }
    Ok(())
}

/// Everything `handle_notification` needs besides the notification
/// itself — bundled purely to stay under clippy's argument-count ceiling;
/// same fields, same mutability, same lifetimes as before, no behavior
/// change.
struct NotificationCtx<'a> {
    db: &'a mut Db,
    files: &'a mut HashMap<PathBuf, SourceFile>,
    open: &'a mut HashMap<PathBuf, i32>,
    connection: &'a Connection,
    work_tx: &'a Sender<RecomputeRequest>,
    root: &'a ProjectRoot,
    warned: &'a mut HashSet<PathBuf>,
    declarations_only: &'a HashSet<PathBuf>,
}

fn handle_notification(note: lsp_server::Notification, ctx: &mut NotificationCtx) -> Result<()> {
    let method = note.method.clone();
    match method.as_str() {
        m if m == <DidOpenTextDocument as NotificationTrait>::METHOD => {
            let params: DidOpenTextDocumentParams = serde_json::from_value(note.params)?;
            let Some(path) = uri_to_path(&params.text_document.uri) else {
                return Ok(());
            };
            if !ctx.root.contains(&path) {
                if ctx.warned.insert(path.clone()) {
                    send_out_of_root_warning(ctx.connection, &path, &ctx.root.original)?;
                }
                return Ok(());
            }
            let version = params.text_document.version;
            let text = params.text_document.text;
            if let Some(file) = ctx.files.get(&path) {
                file.set_text(&mut *ctx.db).to(text);
            } else {
                let file = SourceFile::new(&*ctx.db, path.clone(), text);
                ctx.files.insert(path.clone(), file);
                let all: Vec<SourceFile> = ctx.files.values().copied().collect();
                match ProjectFiles::try_get(&*ctx.db) {
                    Some(project) => {
                        project.set_files(&mut *ctx.db).to(all);
                    }
                    None => {
                        ProjectFiles::new(&*ctx.db, all);
                    }
                }
            }
            ctx.open.insert(path, version);
            schedule_recompute(&*ctx.db, ctx.files, ctx.open, ctx.work_tx, ctx.declarations_only);
        }
        m if m == <DidChangeTextDocument as NotificationTrait>::METHOD => {
            let params: DidChangeTextDocumentParams = serde_json::from_value(note.params)?;
            let Some(path) = uri_to_path(&params.text_document.uri) else {
                return Ok(());
            };
            let Some(change) = params.content_changes.into_iter().last() else {
                return Ok(());
            };
            if let Some(file) = ctx.files.get(&path) {
                file.set_text(&mut *ctx.db).to(change.text);
                ctx.open.insert(path, params.text_document.version);
                schedule_recompute(&*ctx.db, ctx.files, ctx.open, ctx.work_tx, ctx.declarations_only);
            }
        }
        m if m == <DidCloseTextDocument as NotificationTrait>::METHOD => {
            let params: DidCloseTextDocumentParams = serde_json::from_value(note.params)?;
            let Some(path) = uri_to_path(&params.text_document.uri) else {
                return Ok(());
            };
            if !ctx.files.contains_key(&path) {
                return Ok(());
            }
            ctx.open.remove(&path);
            let empty = PublishDiagnosticsParams {
                uri: params.text_document.uri,
                diagnostics: Vec::new(),
                version: None,
            };
            let note = lsp_server::Notification::new(
                <PublishDiagnostics as NotificationTrait>::METHOD.to_string(),
                empty,
            );
            ctx.connection.sender.send(Message::Notification(note))?;
        }
        m if m == <DidSaveTextDocument as NotificationTrait>::METHOD => {
            // No content is carried on save; the prior didChange already updated the input.
        }
        _ => {
            // Unhandled notifications (e.g. $/cancelRequest, workspace/didChangeWatchedFiles
            // for non-open files) are silently ignored, per the LSP spec.
        }
    }
    Ok(())
}

/// One recomputation request: a `Db` snapshot plus the open files to check.
/// The worker drains the channel down to the newest of these before running.
struct RecomputeRequest {
    db: Db,
    open: Vec<(SourceFile, Uri, i32)>,
}

fn schedule_recompute(
    db: &Db,
    files: &HashMap<PathBuf, SourceFile>,
    open: &HashMap<PathBuf, i32>,
    work_tx: &Sender<RecomputeRequest>,
    declarations_only: &HashSet<PathBuf>,
) {
    let mut items = Vec::with_capacity(open.len());
    for (path, version) in open {
        if declarations_only.contains(path) {
            // bead ita-16g contract #5: a declaration-only source (e.g. a
            // discovered `db/schema.rb`) never gets diagnostics published,
            // even if the client opens it directly — it's a declaration,
            // not code under review, exactly like `ita check`'s own
            // `declarations_only` filter.
            continue;
        }
        let Some(file) = files.get(path) else { continue };
        let Ok(uri) = path_to_uri(path) else { continue };
        items.push((*file, uri, *version));
    }
    // The receiver end never disappears before the sender is dropped in `run`;
    // a send error here just means the worker already exited, safe to ignore.
    let _ = work_tx.send(RecomputeRequest {
        db: db.clone(),
        open: items,
    });
}

/// Runs `check_file` on `file` and converts the results to LSP diagnostics
/// (range/severity/code) — the one conversion the `publishDiagnostics`
/// worker and the `textDocument/diagnostic` pull handler (bead ita-0fh)
/// both use, so the wire shape can never drift between the two paths.
/// `None` means the query was cancelled by a concurrent edit — never an
/// empty list, so a caller that cares (the worker skips publishing rather
/// than clearing) can tell "computed nothing" apart from "not computed".
fn file_lsp_diagnostics(db: &Db, file: SourceFile) -> Option<Vec<LspDiagnostic>> {
    let diagnostics = match salsa::Cancelled::catch(AssertUnwindSafe(|| check_file(db, file))) {
        Ok(diagnostics) => diagnostics,
        Err(_cancelled) => return None,
    };
    let text = file.text(db);
    let index = LineIndex::new(text);
    Some(diagnostics.iter().map(|d| to_lsp_diagnostic(text, &index, d)).collect())
}

fn diagnostics_worker(work_rx: &crossbeam_channel::Receiver<RecomputeRequest>, sender: &Sender<Message>) {
    'outer: loop {
        let Ok(mut req) = work_rx.recv() else {
            return;
        };
        // Coalesce: only the newest snapshot matters, drop everything older.
        while let Ok(newer) = work_rx.try_recv() {
            req = newer;
        }

        for (file, uri, version) in &req.open {
            let db = &req.db;
            let Some(lsp_diagnostics) = file_lsp_diagnostics(db, *file) else {
                // A fresher snapshot is already on its way; go fetch it —
                // never publish an empty list in place of "not computed".
                continue 'outer;
            };

            let params = PublishDiagnosticsParams {
                uri: uri.clone(),
                diagnostics: lsp_diagnostics,
                version: Some(*version),
            };
            let note = lsp_server::Notification::new(
                <PublishDiagnostics as NotificationTrait>::METHOD.to_string(),
                params,
            );
            if sender.send(Message::Notification(note)).is_err() {
                return;
            }
        }
    }
}

fn to_lsp_diagnostic(text: &str, index: &LineIndex, diag: &SemDiagnostic) -> LspDiagnostic {
    let (start_line, start_col) = index.line_col(text, diag.start);
    let (end_line, end_col) = index.line_col(text, diag.end);
    LspDiagnostic {
        range: Range {
            start: Position::new(start_line, start_col),
            end: Position::new(end_line, end_col),
        },
        severity: Some(match diag.severity {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
        }),
        code: Some(NumberOrString::String(diag.code.to_string())),
        code_description: None,
        source: Some("itaruby".to_string()),
        message: diag.message.clone(),
        related_information: None,
        tags: None,
        data: None,
    }
}

#[allow(deprecated)]
fn workspace_root(params: &InitializeParams) -> PathBuf {
    if let Some(folders) = &params.workspace_folders {
        if let Some(folder) = folders.first() {
            if let Some(path) = uri_to_path(&folder.uri) {
                return path;
            }
        }
    }
    if let Some(uri) = &params.root_uri {
        if let Some(path) = uri_to_path(uri) {
            return path;
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn discover_rb_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in ignore::WalkBuilder::new(root).build() {
        let Ok(entry) = entry else { continue };
        let is_file = entry.file_type().is_some_and(|t| t.is_file());
        let is_rb = entry.path().extension().and_then(|e| e.to_str()) == Some("rb");
        if is_file && is_rb {
            out.push(entry.path().to_path_buf());
        }
    }
    out
}

// `lsp-types` 0.97 represents `Uri` as a strict `fluent_uri::Uri<String>` with
// no OS-path conversion helpers (unlike the older `url::Url`). These two
// functions are the whole of that missing glue: percent-encode/decode a
// `file://` URI's path component ourselves.

fn path_to_uri(path: &Path) -> anyhow::Result<Uri> {
    let raw = path.to_string_lossy();
    let encoded = percent_encode_path(&raw);
    let uri_str = if encoded.starts_with('/') {
        format!("file://{encoded}")
    } else {
        format!("file:///{encoded}")
    };
    uri_str
        .parse::<Uri>()
        .map_err(|e| anyhow::anyhow!("invalid file uri for {}: {e}", path.display()))
}

fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    let scheme = uri.scheme()?;
    if !scheme.eq_lowercase("file") {
        return None;
    }
    Some(PathBuf::from(percent_decode(uri.path().as_str())))
}

fn percent_encode_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'~'
            | b'/'
            | b':'
            | b'@'
            | b'!'
            | b'$'
            | b'&'
            | b'\''
            | b'('
            | b')'
            | b'*'
            | b'+'
            | b','
            | b';'
            | b'=' => out.push(b as char),
            _ => write!(out, "%{b:02X}").expect("write to String never fails"),
        }
    }
    out
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct TempDirGuard(PathBuf);
    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("itaruby-server-unit-{name}-{}", std::process::id()))
    }

    /// The macOS trap `contains`'s doc comment exists for: `/tmp` is itself
    /// a symlink to `/private/tmp`, so a client-reported root and a
    /// client-reported file path can be textually unrelated yet the same
    /// directory on disk. `contains` MUST resolve through the symlink via
    /// `canonicalize`, not compare the unresolved paths.
    #[test]
    fn contains_resolves_symlinked_root_via_canonicalize() -> anyhow::Result<()> {
        let base = tmp("symlink-root");
        let _guard = TempDirGuard(base.clone());
        let real = base.join("real");
        let link = base.join("link");
        fs::create_dir_all(&real)?;
        std::os::unix::fs::symlink(&real, &link)?;
        fs::write(real.join("foo.rb"), "")?;

        let root = ProjectRoot::new(&link);
        // The file exists only under `real`; opened via the symlinked root
        // path it must still canonicalize to the same file and be contained.
        assert!(root.contains(&link.join("foo.rb")));
        Ok(())
    }

    /// `contains` falls back to a component-wise lexical comparison for a
    /// path that does not exist yet (`canonicalize` fails). That fallback
    /// MUST NOT degrade into a byte-prefix check: `root-evil` shares a
    /// string prefix with `root` but is not under it.
    #[test]
    fn contains_fallback_is_component_wise_not_substring() -> anyhow::Result<()> {
        let base = tmp("lexical-fallback");
        let _guard = TempDirGuard(base.clone());
        let root_dir = base.join("root");
        fs::create_dir_all(&root_dir)?;

        let root = ProjectRoot::new(&root_dir);
        // Not yet created on disk -> canonicalize fails -> lexical fallback.
        assert!(root.contains(&root_dir.join("new_subdir").join("new_file.rb")));
        let sibling_evil = base.join("root-evil").join("new_file.rb");
        assert!(!root.contains(&sibling_evil));
        Ok(())
    }

    /// `percent_encode_path` MUST escape characters that are significant in
    /// a `file://` URI (space, `#`) — decoding a pass-through space would
    /// still round-trip, so the round trip alone can't catch a missing
    /// encode; assert the wire form directly too.
    #[test]
    fn percent_round_trip_encodes_reserved_characters() {
        let path = "/Users/x/my project/notes#1.rb";
        let encoded = percent_encode_path(path);
        assert!(!encoded.contains(' '), "space must be percent-encoded: {encoded}");
        assert!(!encoded.contains('#'), "'#' must be percent-encoded: {encoded}");
        assert_eq!(percent_decode(&encoded), path);
    }
}
