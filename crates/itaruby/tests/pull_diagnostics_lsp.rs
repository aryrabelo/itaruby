//! End-to-end LSP tests for `textDocument/diagnostic` pull diagnostics
//! (bead ita-0fh, C1): the LSP 3.17 standard request an editor uses to ask
//! for a file's diagnostics synchronously, instead of waiting for the
//! `publishDiagnostics` worker's next push. Same minimal hand-rolled
//! JSON-RPC framing as `hover_lsp.rs`/`containment_lsp.rs`: spawn `ita
//! server`, speak raw JSON-RPC over its stdio, prove what an editor
//! actually sees on the wire.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

fn read_message(reader: &mut impl BufRead) -> Option<Value> {
    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).ok()?;
        if n == 0 {
            return None;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(rest) = line.strip_prefix("Content-Length:") {
            content_length = rest.trim().parse().ok();
        }
    }
    let len = content_length?;
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).ok()?;
    serde_json::from_slice(&buf).ok()
}

fn write_message(writer: &mut impl Write, msg: &Value) {
    let body = serde_json::to_vec(msg).expect("serialize lsp message");
    write!(writer, "Content-Length: {}\r\n\r\n", body.len()).expect("write header");
    writer.write_all(&body).expect("write body");
    writer.flush().expect("flush");
}

/// Several messages in ONE write: the pipe hands the server the whole
/// burst before it can process any of it — needed for the "cancel already
/// queued when its request arrives" timing test.
fn write_burst(writer: &mut impl Write, msgs: &[Value]) {
    let mut buf = Vec::new();
    for msg in msgs {
        let body = serde_json::to_vec(msg).expect("serialize lsp message");
        write!(buf, "Content-Length: {}\r\n\r\n", body.len()).expect("write header");
        buf.extend_from_slice(&body);
    }
    writer.write_all(&buf).expect("write burst");
    writer.flush().expect("flush");
}

/// Best-effort recursive delete on drop, even if an assertion panics.
struct TempDirGuard(PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct LspClient {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<Value>,
    next_id: i64,
}

impl LspClient {
    fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ita"))
            .arg("server")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn `ita server`");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = child.stdout.take().expect("child stdout");

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            while let Some(msg) = read_message(&mut reader) {
                if tx.send(msg).is_err() {
                    break;
                }
            }
        });

        Self { child, stdin, rx, next_id: 1 }
    }

    fn request(&mut self, method: &str, params: &Value) -> i64 { let id = self.next_id;
    self.next_id += 1;
    write_message(
        &mut self.stdin,
        &json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }),
    );
    id }

    fn notify(&mut self, method: &str, params: &Value) { write_message(&mut self.stdin, &json!({ "jsonrpc": "2.0", "method": method, "params": params })); }

    fn wait_response(&self, id: i64, timeout: Duration) -> Value {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "timed out after {timeout:?} waiting for response id={id}");
            let msg = self
                .rx
                .recv_timeout(remaining)
                .unwrap_or_else(|_| panic!("timed out after {timeout:?} waiting for response id={id}"));
            if msg.get("id").and_then(Value::as_i64) == Some(id) {
                return msg;
            }
        }
    }

    fn wait_exit(&mut self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        loop {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            if Instant::now() >= deadline {
                let _ = self.child.kill();
                let _ = self.child.wait();
                panic!("`ita server` did not exit within {timeout:?} of `exit`; killed it");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

fn file_uri(path: &std::path::Path) -> String {
    format!("file://{}", path.display())
}

const LONG: Duration = Duration::from_secs(30);

fn init_and_open_root(client: &mut LspClient, root: &std::path::Path) -> Value {
    let init_id = client.request("initialize", &json!({ "processId": null, "rootUri": file_uri(root), "capabilities": {} }));
    let init_response = client.wait_response(init_id, LONG);
    assert!(init_response.get("error").is_none(), "initialize returned an error: {init_response:?}");
    client.notify("initialized", &json!({}));
    init_response
}

fn pull_diagnostics(client: &mut LspClient, uri: &str) -> Value {
    let id = client.request("textDocument/diagnostic", &json!({ "textDocument": { "uri": uri } }));
    client.wait_response(id, LONG)
}

fn shutdown(client: &mut LspClient) {
    let shutdown_id = client.request("shutdown", &Value::Null);
    client.wait_response(shutdown_id, LONG);
    client.notify("exit", &Value::Null);
    client.wait_exit(Duration::from_secs(5));
}

/// (a) `initialize` must advertise `diagnosticProvider`, and a pull for a
/// file with a typo'd method call must answer a `RelatedFullDocumentDiagnosticReport`
/// (`kind: "full"`) carrying exactly the E0101 the checker itself would
/// report — same code, same Error severity, same non-degenerate range
/// (`check_file`'s message span covers the identifier, so start != end;
/// the publish path already computes that same real end position via
/// `LineIndex::line_col(diag.end)`, and pull shares the exact conversion).
#[test]
fn pull_diagnostics_reports_full_report_with_one_item() {
    let dir = std::env::temp_dir().join(format!("itaruby-pull-diag-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let _guard = TempDirGuard(dir.clone());

    let path = dir.join("typo.rb");
    let text = "class User\n  def name\n    \"x\"\n  end\nend\n\nUser.new.nmae\n";
    std::fs::write(&path, text).unwrap();

    let mut client = LspClient::spawn();
    let init_response = init_and_open_root(&mut client, &dir);
    assert!(
        init_response["result"]["capabilities"]["diagnosticProvider"].is_object(),
        "server must advertise diagnosticProvider, got: {init_response:?}"
    );
    assert_eq!(
        init_response["result"]["capabilities"]["diagnosticProvider"]["identifier"],
        json!("itaruby"),
    );

    let uri = file_uri(&path);
    client.notify("textDocument/didOpen", &json!({
        "textDocument": { "uri": uri, "languageId": "ruby", "version": 1, "text": text }
    }));

    let resp = pull_diagnostics(&mut client, &uri);
    assert!(resp.get("error").is_none(), "pull returned an error: {resp:?}");
    assert_eq!(resp["result"]["kind"], json!("full"), "expected a full report, got: {resp:?}");
    let items = resp["result"]["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1, "expected exactly one item, got: {resp:?}");
    let item = &items[0];
    assert_eq!(item["code"], json!("E0101"), "unexpected code: {item:?}");
    assert_eq!(item["severity"], json!(1), "E0101 must be Error (1): {item:?}");
    assert_ne!(
        item["range"]["start"], item["range"]["end"],
        "range must cover the typo'd identifier, not collapse to a point: {item:?}"
    );

    shutdown(&mut client);
}

/// (a2) The same wire path carries E0108 with no per-code wiring in the
/// server: an editor pulling this file sees code `E0108`, severity 1
/// (Error), and a range covering the offending OPERAND. The snippet is
/// the capability's reference case, a real MRI `TypeError`.
#[test]
fn pull_diagnostics_reports_operand_type_mismatch() {
    let dir = std::env::temp_dir().join(format!("itaruby-pull-operand-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let _guard = TempDirGuard(dir.clone());

    let path = dir.join("price.rb");
    let text = "price = 100\nlabel = \"R$ #{price}\"\nprice + label\n";
    std::fs::write(&path, text).unwrap();

    let mut client = LspClient::spawn();
    init_and_open_root(&mut client, &dir);

    let uri = file_uri(&path);
    client.notify("textDocument/didOpen", &json!({
        "textDocument": { "uri": uri, "languageId": "ruby", "version": 1, "text": text }
    }));

    let resp = pull_diagnostics(&mut client, &uri);
    assert!(resp.get("error").is_none(), "pull returned an error: {resp:?}");
    let items = resp["result"]["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1, "expected exactly one item, got: {resp:?}");
    let item = &items[0];
    assert_eq!(item["code"], json!("E0108"), "unexpected code: {item:?}");
    assert_eq!(item["severity"], json!(1), "E0108 must be Error (1): {item:?}");
    assert_eq!(
        item["message"],
        json!("`+` on Integer expects a numeric operand, got String"),
        "unexpected message: {item:?}"
    );
    // Line 2 (0-based), columns 8..13 — the `label` operand, not the
    // whole expression and not a collapsed point.
    assert_eq!(item["range"]["start"], json!({ "line": 2, "character": 8 }), "got: {item:?}");
    assert_eq!(item["range"]["end"], json!({ "line": 2, "character": 13 }), "got: {item:?}");

    shutdown(&mut client);
}

/// (b) `didChange` fixing the typo, then a fresh pull, must answer empty
/// items — the drain-before-respond queue absorbs the pending edit before
/// dispatch runs, so pull never answers about stale text.
#[test]
fn pull_diagnostics_reflects_did_change() {
    let dir = std::env::temp_dir().join(format!("itaruby-pull-diag-change-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let _guard = TempDirGuard(dir.clone());

    let path = dir.join("typo.rb");
    let text = "class User\n  def name\n    \"x\"\n  end\nend\n\nUser.new.nmae\n";
    std::fs::write(&path, text).unwrap();

    let mut client = LspClient::spawn();
    init_and_open_root(&mut client, &dir);

    let uri = file_uri(&path);
    client.notify("textDocument/didOpen", &json!({
        "textDocument": { "uri": uri, "languageId": "ruby", "version": 1, "text": text }
    }));

    let resp = pull_diagnostics(&mut client, &uri);
    let items = resp["result"]["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1, "expected the typo before the fix, got: {resp:?}");

    let fixed = "class User\n  def name\n    \"x\"\n  end\nend\n\nUser.new.name\n";
    client.notify("textDocument/didChange", &json!({
        "textDocument": { "uri": uri, "version": 2 },
        "contentChanges": [{ "text": fixed }]
    }));

    let resp = pull_diagnostics(&mut client, &uri);
    assert!(resp.get("error").is_none(), "pull returned an error: {resp:?}");
    let items = resp["result"]["items"].as_array().expect("items array");
    assert!(items.is_empty(), "expected the fix to clear E0101, got: {resp:?}");

    shutdown(&mut client);
}

/// (c) A pull for a `uri` the server never admitted (outside the project
/// root, never opened) must answer an empty full report — never an error,
/// never a crash.
#[test]
fn pull_diagnostics_for_unknown_uri_is_empty() {
    let dir = std::env::temp_dir().join(format!("itaruby-pull-diag-unknown-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let _guard = TempDirGuard(dir.clone());

    let outside_dir =
        std::env::temp_dir().join(format!("itaruby-pull-diag-outside-{}", std::process::id()));
    std::fs::create_dir_all(&outside_dir).unwrap();
    let _outside_guard = TempDirGuard(outside_dir.clone());
    let outside_path = outside_dir.join("never_opened.rb");
    std::fs::write(&outside_path, "1\n").unwrap();

    let mut client = LspClient::spawn();
    init_and_open_root(&mut client, &dir);

    let resp = pull_diagnostics(&mut client, &file_uri(&outside_path));
    assert!(resp.get("error").is_none(), "pull returned an error: {resp:?}");
    assert_eq!(resp["result"]["kind"], json!("full"), "expected a full report, got: {resp:?}");
    let items = resp["result"]["items"].as_array().expect("items array");
    assert!(items.is_empty(), "unknown uri must report no diagnostics, got: {resp:?}");

    shutdown(&mut client);
}

/// (d) A pull for a discovered `db/schema.rb` (declarations-only, bead
/// ita-16g — never code under review) must answer empty items even though
/// the file is loaded into the project at startup.
#[test]
fn pull_diagnostics_for_declarations_only_file_is_empty() {
    let dir = std::env::temp_dir().join(format!("itaruby-pull-diag-schema-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let _guard = TempDirGuard(dir.clone());

    std::fs::create_dir_all(dir.join("db")).unwrap();
    let schema_path = dir.join("db/schema.rb");
    std::fs::write(
        &schema_path,
        "ActiveRecord::Schema[7.1].define(version: 1) do\n  create_table \"widgets\", force: :cascade do |t|\n    t.string \"title\"\n  end\nend\n",
    )
    .unwrap();

    let mut client = LspClient::spawn();
    init_and_open_root(&mut client, &dir);

    let resp = pull_diagnostics(&mut client, &file_uri(&schema_path));
    assert!(resp.get("error").is_none(), "pull returned an error: {resp:?}");
    assert_eq!(resp["result"]["kind"], json!("full"), "expected a full report, got: {resp:?}");
    let items = resp["result"]["items"].as_array().expect("items array");
    assert!(items.is_empty(), "db/schema.rb is declarations-only, must report no diagnostics: {resp:?}");

    shutdown(&mut client);
}

/// (e) A pull cancelled while still queued must answer `RequestCancelled`
/// (-32800), and the server must keep serving pulls afterwards. Cancel and
/// request are written in one burst so the server has both in hand before
/// dispatching (same technique as `hover_lsp.rs`'s cancellation test).
#[test]
fn pull_diagnostics_cancel_request_answers_request_cancelled() {
    let dir = std::env::temp_dir().join(format!("itaruby-pull-diag-cancel-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let _guard = TempDirGuard(dir.clone());

    let path = dir.join("cancellable.rb");
    let text = "class User\n  def name\n    \"x\"\n  end\nend\n\nUser.new.nmae\n";
    std::fs::write(&path, text).unwrap();

    let mut client = LspClient::spawn();
    init_and_open_root(&mut client, &dir);

    let uri = file_uri(&path);
    client.notify("textDocument/didOpen", &json!({
        "textDocument": { "uri": uri, "languageId": "ruby", "version": 1, "text": text }
    }));
    // Synchronize: this pull's response proves the server drained the
    // channel, so the burst below is the only thing in flight.
    let sync_id = client.request("textDocument/diagnostic", &json!({ "textDocument": { "uri": uri } }));
    let _ = client.wait_response(sync_id, LONG);

    let cancel = json!({ "jsonrpc": "2.0", "method": "$/cancelRequest", "params": { "id": 41 } });
    let pull = json!({
        "jsonrpc": "2.0",
        "id": 41,
        "method": "textDocument/diagnostic",
        "params": { "textDocument": { "uri": uri } }
    });
    write_burst(&mut client.stdin, &[cancel, pull]);
    let resp = client.wait_response(41, LONG);
    assert_eq!(
        resp["error"]["code"],
        json!(-32800),
        "cancelled pull must answer RequestCancelled, got: {resp:?}"
    );

    // The server is still healthy: a fresh (uncancelled) pull answers.
    let resp = pull_diagnostics(&mut client, &uri);
    assert!(resp.get("error").is_none(), "post-cancel pull errored: {resp:?}");
    let items = resp["result"]["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1, "post-cancel pull must still see the typo, got: {resp:?}");

    shutdown(&mut client);
}
