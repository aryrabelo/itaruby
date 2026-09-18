//! End-to-end LSP tests for `textDocument/hover` and request cancellation
//! under fast `didChange` (w4). Same minimal hand-rolled JSON-RPC framing
//! as `definition_lsp.rs`: spawn `ita server`, speak raw JSON-RPC over its
//! stdio, prove what an editor actually sees on the wire.

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
/// burst before it can process any of it, which is exactly the "fast
/// didChange" / "request already queued when its cancel arrives" timing
/// these tests need — a send-per-message would race the server's loop.
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

    fn request(&mut self, method: &str, params: &Value) -> (i64, Value) { let id = self.next_id;
    self.next_id += 1;
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
    write_message(&mut self.stdin, &msg);
    (id, msg) }

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

/// initialize → didOpen → known-type hover → honest null hover →
/// didChange → hover reflects the NEW text. Also proves `hoverProvider`
/// is advertised.
#[test]
fn hover_over_lsp() {
    let dir = std::env::temp_dir().join(format!("itaruby-hover-lsp-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let _guard = TempDirGuard(dir.clone());

    let path = dir.join("hovered.rb");
    let text = "msg = \"hello\"\n";
    std::fs::write(&path, text).unwrap();

    let mut client = LspClient::spawn();

    let init_id = client.request("initialize", &json!({
        "processId": null,
        "rootUri": file_uri(&dir),
        "capabilities": {},
    }));
    let init_response = client.wait_response(init_id.0, LONG);
    assert!(
        init_response.get("error").is_none(),
        "initialize returned an error: {init_response:?}"
    );
    assert_eq!(
        init_response["result"]["capabilities"]["hoverProvider"],
        json!(true),
        "server must advertise hoverProvider, got: {init_response:?}"
    );
    client.notify("initialized", &json!({}));

    let uri = file_uri(&path);
    client.notify("textDocument/didOpen", &json!({
        "textDocument": { "uri": uri, "languageId": "ruby", "version": 1, "text": text }
    }));

    // (0,0) sits on `msg`, typed String by its literal.
    let (hover_id, _) = client.request("textDocument/hover", &json!({
        "textDocument": { "uri": uri },
        "position": { "line": 0, "character": 0 },
    }));
    let resp = client.wait_response(hover_id, LONG);
    assert!(resp.get("error").is_none(), "hover returned an error: {resp:?}");
    let contents = &resp["result"]["contents"]["value"];
    assert_eq!(contents, &json!("```ruby\nString\n```"), "hover on a String literal local: {resp:?}");

    // didChange to an Integer literal: the SAME position must now hover
    // Integer — the answer tracks the freshest revision, never the text
    // the client already replaced.
    client.notify("textDocument/didChange", &json!({
        "textDocument": { "uri": uri, "version": 2 },
        "contentChanges": [{ "text": "msg = 42\n" }]
    }));
    let (hover_id, _) = client.request("textDocument/hover", &json!({
        "textDocument": { "uri": uri },
        "position": { "line": 0, "character": 0 },
    }));
    let resp = client.wait_response(hover_id, LONG);
    assert!(resp.get("error").is_none(), "hover returned an error: {resp:?}");
    assert_eq!(
        resp["result"]["contents"]["value"],
        json!("```ruby\nInteger\n```"),
        "hover after didChange must answer about the new text: {resp:?}"
    );

    // Past the end of the (one-line) document: no info, so `null` — never
    // a guess.
    let (hover_id, _) = client.request("textDocument/hover", &json!({
        "textDocument": { "uri": uri },
        "position": { "line": 5, "character": 0 },
    }));
    let resp = client.wait_response(hover_id, LONG);
    assert!(resp.get("error").is_none(), "hover returned an error: {resp:?}");
    assert_eq!(resp["result"], Value::Null, "out-of-range position: expected null, got: {resp:?}");

    let shutdown_id = client.request("shutdown", &Value::Null);
    client.wait_response(shutdown_id.0, LONG);
    client.notify("exit", &Value::Null);
    client.wait_exit(Duration::from_secs(5));
}

/// A request cancelled while still queued must answer `RequestCancelled`
/// (-32800), and the server must keep serving afterwards. The request and
/// its cancel are written in one burst so the server has both in hand
/// before dispatching.
#[test]
fn cancel_request_answers_request_cancelled() {
    let dir = std::env::temp_dir().join(format!("itaruby-hover-cancel-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let _guard = TempDirGuard(dir.clone());

    let path = dir.join("cancellable.rb");
    let text = "msg = \"hello\"\n";
    std::fs::write(&path, text).unwrap();

    let mut client = LspClient::spawn();
    let init_id = client.request("initialize", &json!({ "processId": null, "rootUri": file_uri(&dir), "capabilities": {} }));
    client.wait_response(init_id.0, LONG);
    client.notify("initialized", &json!({}));

    let uri = file_uri(&path);
    client.notify("textDocument/didOpen", &json!({
        "textDocument": { "uri": uri, "languageId": "ruby", "version": 1, "text": text }
    }));
    // Synchronize: this hover's response proves the server drained the
    // channel, so the burst below is the only thing in flight.
    let (sync_id, _) = client.request("textDocument/hover", &json!({
        "textDocument": { "uri": uri },
        "position": { "line": 0, "character": 0 },
    }));
    let _ = client.wait_response(sync_id, LONG);

    // Cancel first, request second — one burst. LSP cancellation is
    // best-effort by spec; what the server CONTRACTS is "a cancel seen
    // before dispatch ⇒ RequestCancelled". The cancel-first order makes
    // that deterministic (channel order guarantees the cancel is absorbed
    // before the request is dispatched), unlike request-first, where the
    // reader thread may not have pushed the cancel yet when the request
    // is popped — same code path, no race.
    let cancel = json!({ "jsonrpc": "2.0", "method": "$/cancelRequest", "params": { "id": 41 } });
    let hover = json!({
        "jsonrpc": "2.0",
        "id": 41,
        "method": "textDocument/hover",
        "params": {
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 0 },
        }
    });
    write_burst(&mut client.stdin, &[cancel, hover]);
    let resp = client.wait_response(41, LONG);
    assert_eq!(
        resp["error"]["code"],
        json!(-32800),
        "cancelled hover must answer RequestCancelled, got: {resp:?}"
    );

    // The server is still healthy: a fresh (uncancelled) hover answers.
    let (fresh_id, _) = client.request("textDocument/hover", &json!({
        "textDocument": { "uri": uri },
        "position": { "line": 0, "character": 0 },
    }));
    let resp = client.wait_response(fresh_id, LONG);
    assert!(resp.get("error").is_none(), "post-cancel hover errored: {resp:?}");
    assert_eq!(resp["result"]["contents"]["value"], json!("```ruby\nString\n```"));

    let shutdown_id = client.request("shutdown", &Value::Null);
    client.wait_response(shutdown_id.0, LONG);
    client.notify("exit", &Value::Null);
    client.wait_exit(Duration::from_secs(5));
}

/// Fast didChange: a hover queued BEHIND a didChange (one burst) must
/// answer about the NEW text — no stale answer may leak out of the queue.
#[test]
fn rapid_did_change_never_answers_stale_text() {
    let dir = std::env::temp_dir().join(format!("itaruby-hover-rapid-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let _guard = TempDirGuard(dir.clone());

    let path = dir.join("rapid.rb");
    let text = "msg = \"hello\"\n";
    std::fs::write(&path, text).unwrap();

    let mut client = LspClient::spawn();
    let init_id = client.request("initialize", &json!({ "processId": null, "rootUri": file_uri(&dir), "capabilities": {} }));
    client.wait_response(init_id.0, LONG);
    client.notify("initialized", &json!({}));

    let uri = file_uri(&path);
    client.notify("textDocument/didOpen", &json!({
        "textDocument": { "uri": uri, "languageId": "ruby", "version": 1, "text": text }
    }));

    // didChange v2 then hover, one burst. The drain-before-respond rule
    // applies the didChange before the hover is answered.
    let did_change = json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didChange",
        "params": {
            "textDocument": { "uri": uri, "version": 2 },
            "contentChanges": [{ "text": "msg = 42\n" }]
        }
    });
    let hover = json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "textDocument/hover",
        "params": {
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 0 },
        }
    });
    write_burst(&mut client.stdin, &[did_change, hover]);
    let resp = client.wait_response(7, LONG);
    assert!(resp.get("error").is_none(), "hover returned an error: {resp:?}");
    assert_eq!(
        resp["result"]["contents"]["value"],
        json!("```ruby\nInteger\n```"),
        "hover queued behind a didChange must answer about the new text, got: {resp:?}"
    );

    let shutdown_id = client.request("shutdown", &Value::Null);
    client.wait_response(shutdown_id.0, LONG);
    client.notify("exit", &Value::Null);
    client.wait_exit(Duration::from_secs(5));
}
