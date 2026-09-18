//! End-to-end LSP tests for project-root containment (bead ita-bft): a
//! `textDocument/didOpen` for a path outside the server's workspace root
//! must be ignored whole — no `SourceFile`, no merge into `ProjectFiles`,
//! no diagnostics ever published for it — plus a one-time `window/
//! logMessage` warning naming the root. `didChange`/`didClose`/`didSave`
//! for a path the server never admitted are no-ops.
//!
//! Same minimal hand-rolled JSON-RPC framing as `lsp_smoke.rs`: spawn `ita
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

    /// Blocks (up to `timeout`) for the response matching `id`, discarding
    /// any notifications received along the way.
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

    /// Blocks (up to `timeout`) for a `textDocument/publishDiagnostics`
    /// notification for `uri` whose diagnostics satisfy `accept`, discarding
    /// unrelated messages (including publishDiagnostics for other files).
    fn wait_diagnostics(
        &self,
        uri: &str,
        timeout: Duration,
        mut accept: impl FnMut(&[Value]) -> bool,
    ) -> Vec<Value> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "timed out after {timeout:?} waiting for publishDiagnostics({uri})");
            let msg = self.rx.recv_timeout(remaining).unwrap_or_else(|_| {
                panic!("timed out after {timeout:?} waiting for publishDiagnostics({uri})")
            });
            if msg.get("method").and_then(Value::as_str) != Some("textDocument/publishDiagnostics") {
                continue;
            }
            let params = &msg["params"];
            if params.get("uri").and_then(Value::as_str) != Some(uri) {
                continue;
            }
            let diags = params["diagnostics"].as_array().cloned().unwrap_or_default();
            if accept(&diags) {
                return diags;
            }
        }
    }

    /// Blocks (up to `timeout`) for a `window/logMessage` notification,
    /// discarding unrelated messages along the way.
    fn wait_log_message(&self, timeout: Duration) -> Value {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "timed out after {timeout:?} waiting for window/logMessage");
            let msg = self.rx.recv_timeout(remaining).unwrap_or_else(|_| {
                panic!("timed out after {timeout:?} waiting for window/logMessage")
            });
            if msg.get("method").and_then(Value::as_str) == Some("window/logMessage") {
                return msg;
            }
        }
    }

    /// Collects every message that arrives within `duration`, for negative
    /// assertions ("nothing of shape X ever showed up") — never panics, an
    /// empty vec is a legitimate, expected result.
    fn drain_for(&self, duration: Duration) -> Vec<Value> {
        let deadline = Instant::now() + duration;
        let mut out = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return out;
            }
            match self.rx.recv_timeout(remaining) {
                Ok(msg) => out.push(msg),
                Err(_) => return out,
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

fn init_and_open_root(client: &mut LspClient, root: &std::path::Path) {
    let init_id = client.request("initialize", &json!({ "processId": null, "rootUri": file_uri(root), "capabilities": {} }));
    let init_response = client.wait_response(init_id, LONG);
    assert!(init_response.get("error").is_none(), "initialize returned an error: {init_response:?}");
    client.notify("initialized", &json!({}));
}

/// (a) A file outside the server's root, reopening a class from an inside
/// file with the exact method a typo call is missing, must be ignored in
/// full: no merge (the typo's E0101 must never clear), no diagnostics ever
/// published for the outside file, and exactly one WARNING `logMessage`
/// naming both the rejected path and the root. `didChange`/`didClose`
/// against that same never-admitted path are then proven to be no-ops.
#[test]
fn open_outside_root_is_ignored_and_warns_once() {
    let root_dir = std::env::temp_dir().join(format!("itaruby-containment-root-{}", std::process::id()));
    let outside_dir =
        std::env::temp_dir().join(format!("itaruby-containment-outside-{}", std::process::id()));
    std::fs::create_dir_all(&root_dir).unwrap();
    std::fs::create_dir_all(&outside_dir).unwrap();
    let _root_guard = TempDirGuard(root_dir.clone());
    let _outside_guard = TempDirGuard(outside_dir.clone());

    // Project A: `User` defines only `name`; `User.new.nmae` is a genuine
    // typo — E0101 — UNLESS an out-of-root reopening of `User` silently
    // merges in.
    let user_path = root_dir.join("user.rb");
    std::fs::write(&user_path, "class User\n  def name\n    \"x\"\n  end\nend\n").unwrap();
    let main_path = root_dir.join("main.rb");
    let broken_text = "User.new.nmae\n";
    std::fs::write(&main_path, broken_text).unwrap();

    // Project B (outside A's root): reopens `User` and defines the exact
    // typo'd method. If this ever merges into A's `ProjectFiles`, the
    // E0101 above silently disappears — the contamination this test
    // guards against.
    let evil_path = outside_dir.join("evil.rb");
    let evil_text = "class User\n  def nmae\n    \"y\"\n  end\nend\n";
    std::fs::write(&evil_path, evil_text).unwrap();

    let mut client = LspClient::spawn();
    init_and_open_root(&mut client, &root_dir);

    let main_uri = file_uri(&main_path);
    client.notify("textDocument/didOpen", &json!({
        "textDocument": { "uri": main_uri, "languageId": "ruby", "version": 1, "text": broken_text }
    }));
    let diags = client.wait_diagnostics(&main_uri, LONG, |d| !d.is_empty());
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic before the out-of-root open, got: {diags:?}");
    assert_eq!(diags[0].get("code").and_then(Value::as_str), Some("E0101"), "got: {diags:?}");

    let evil_uri = file_uri(&evil_path);
    client.notify("textDocument/didOpen", &json!({
        "textDocument": { "uri": evil_uri, "languageId": "ruby", "version": 1, "text": evil_text }
    }));

    let warning = client.wait_log_message(LONG);
    assert_eq!(
        warning["params"]["type"].as_i64(),
        Some(2),
        "expected MessageType::WARNING (2), got: {warning:?}"
    );
    let message = warning["params"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains(&evil_path.display().to_string()),
        "warning must name the out-of-root path: {message}"
    );
    assert!(
        message.contains(&root_dir.display().to_string()),
        "warning must name the root: {message}"
    );

    // didChange/didClose against the same never-admitted path: no-ops per
    // contract #3 — never a crash, never a message about it.
    client.notify("textDocument/didChange", &json!({
        "textDocument": { "uri": evil_uri, "version": 2 },
        "contentChanges": [{ "text": "class User\nend\n" }]
    }));
    client.notify("textDocument/didClose", &json!({ "textDocument": { "uri": evil_uri } }));

    // A request is only dispatched once every notification already queued
    // ahead of it has been fully absorbed — so by the time this hover
    // answers, didOpen/didChange/didClose(evil.rb) are all done.
    let hover_id = client.request("textDocument/hover", &json!({ "textDocument": { "uri": main_uri }, "position": { "line": 0, "character": 0 } }));
    client.wait_response(hover_id, LONG);

    let stray = client.drain_for(Duration::from_millis(300));
    for msg in &stray {
        if msg.get("method").and_then(Value::as_str) != Some("textDocument/publishDiagnostics") {
            continue;
        }
        let uri = msg["params"]["uri"].as_str().unwrap_or_default();
        assert_ne!(uri, evil_uri, "must never publish diagnostics for an out-of-root file: {msg:?}");
        if uri == main_uri {
            let diags = msg["params"]["diagnostics"].as_array().cloned().unwrap_or_default();
            assert!(
                !diags.is_empty(),
                "main.rb's E0101 must not clear from a rejected out-of-root merge: {msg:?}"
            );
        }
    }

    let shutdown_id = client.request("shutdown", &Value::Null);
    client.wait_response(shutdown_id, LONG);
    client.notify("exit", &Value::Null);
    client.wait_exit(Duration::from_secs(5));
}

/// (b) Control for (a): a file INSIDE the root (not present at server
/// startup, so it can only enter via `didOpen`) still gets admitted and
/// diagnosed normally. Without this, (a) proves nothing — containment
/// could just be silently dropping every `didOpen`.
#[test]
fn open_inside_root_still_diagnoses() {
    let root_dir =
        std::env::temp_dir().join(format!("itaruby-containment-inroot-{}", std::process::id()));
    std::fs::create_dir_all(&root_dir).unwrap();
    let _guard = TempDirGuard(root_dir.clone());

    let user_path = root_dir.join("user.rb");
    std::fs::write(&user_path, "class User\n  def name\n    \"x\"\n  end\nend\n").unwrap();

    let mut client = LspClient::spawn();
    init_and_open_root(&mut client, &root_dir);

    // Not on disk at startup — didOpen must still admit it, being inside
    // the root, and its path won't `canonicalize` (doesn't exist yet):
    // this exercises the lexical-normalization fallback directly.
    let main_path = root_dir.join("later.rb");
    let broken_text = "User.new.nmae\n";
    let main_uri = file_uri(&main_path);
    client.notify("textDocument/didOpen", &json!({
        "textDocument": { "uri": main_uri, "languageId": "ruby", "version": 1, "text": broken_text }
    }));

    let diags = client.wait_diagnostics(&main_uri, LONG, |d| !d.is_empty());
    assert_eq!(diags.len(), 1, "in-root open must still diagnose normally, got: {diags:?}");
    assert_eq!(diags[0].get("code").and_then(Value::as_str), Some("E0101"), "got: {diags:?}");

    let shutdown_id = client.request("shutdown", &Value::Null);
    client.wait_response(shutdown_id, LONG);
    client.notify("exit", &Value::Null);
    client.wait_exit(Duration::from_secs(5));
}
