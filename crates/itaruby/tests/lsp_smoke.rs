//! End-to-end LSP smoke test: spawn `ita server`, speak raw JSON-RPC over
//! its stdio, and prove the full cycle: open a file with a typo -> get
//! E0101 -> fix the typo -> diagnostics clear.
//!
//! No lsp-server/lsp-types client helpers here on purpose: this test proves
//! what an editor actually sees on the wire, framing included.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// Reads one `Content-Length: N\r\n\r\n<N bytes of JSON>` frame. `None` on EOF
/// or a malformed frame (treated as stream-closed by the reader thread).
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

#[test]
fn diagnostics_appear_and_clear_on_edit() {
    let dir = std::env::temp_dir().join(format!("itaruby-smoke-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create tempdir");
    let _guard = TempDirGuard(dir.clone());

    let user_path = dir.join("user.rb");
    let main_path = dir.join("main.rb");
    std::fs::write(&user_path, "class User\n  def name\n    \"x\"\n  end\nend\n").unwrap();
    let broken_text = "User.new.nmae\n";
    std::fs::write(&main_path, broken_text).unwrap();

    let mut client = LspClient::spawn();

    let init_id = client.request("initialize", &json!({
        "processId": null,
        "rootUri": file_uri(&dir),
        "capabilities": {},
    }));
    let init_response = client.wait_response(init_id, LONG);
    assert!(
        init_response.get("error").is_none(),
        "initialize returned an error: {init_response:?}"
    );
    client.notify("initialized", &json!({}));

    let main_uri = file_uri(&main_path);
    client.notify("textDocument/didOpen", &json!({
        "textDocument": {
            "uri": main_uri,
            "languageId": "ruby",
            "version": 1,
            "text": broken_text,
        }
    }));

    let diags = client.wait_diagnostics(&main_uri, LONG, |d| !d.is_empty());
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    let d = &diags[0];
    assert_eq!(
        d.get("code").and_then(Value::as_str),
        Some("E0101"),
        "expected E0101, got: {d:?}"
    );
    let range = &d["range"];
    assert_eq!(
        range["start"]["line"].as_u64(),
        Some(0),
        "typo is on line 0 (`User.new.nmae`), got range: {range:?}"
    );

    let fixed_text = "User.new.name\n";
    client.notify("textDocument/didChange", &json!({
        "textDocument": { "uri": main_uri, "version": 2 },
        "contentChanges": [ { "text": fixed_text } ],
    }));

    let diags = client.wait_diagnostics(&main_uri, LONG, <[Value]>::is_empty);
    assert!(diags.is_empty(), "diagnostics should clear after the fix, got: {diags:?}");

    let shutdown_id = client.request("shutdown", &Value::Null);
    client.wait_response(shutdown_id, LONG);
    client.notify("exit", &Value::Null);
    client.wait_exit(Duration::from_secs(5));
}
