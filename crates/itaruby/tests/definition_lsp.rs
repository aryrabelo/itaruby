//! End-to-end LSP smoke test for `textDocument/definition`: spawn `ita
//! server`, speak raw JSON-RPC over its stdio, prove `definitionProvider` is
//! advertised and that a real request resolves to a real `Location`.
//!
//! Same minimal hand-rolled JSON-RPC framing as `lsp_smoke.rs` (no
//! lsp-server/lsp-types client helpers): this proves what an editor actually
//! sees on the wire.

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

#[test]
fn definition_resolves_over_lsp() {
    let dir = std::env::temp_dir().join(format!("itaruby-def-lsp-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let _guard = TempDirGuard(dir.clone());

    let path = dir.join("greeter.rb");
    let text = "class Greeter\n  def greet\n    \"hi\"\n  end\nend\n\nGreeter.new.greet\n";
    std::fs::write(&path, text).unwrap();

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
    assert_eq!(
        init_response["result"]["capabilities"]["definitionProvider"],
        json!(true),
        "server must advertise definitionProvider, got: {init_response:?}"
    );
    client.notify("initialized", &json!({}));

    let uri = file_uri(&path);
    client.notify("textDocument/didOpen", &json!({
        "textDocument": { "uri": uri, "languageId": "ruby", "version": 1, "text": text }
    }));

    // Position 6:14 (0-based line/utf16-col) sits inside `greet` in
    // `Greeter.new.greet` on line 7 (1-based) of the fixture.
    let def_id = client.request("textDocument/definition", &json!({
        "textDocument": { "uri": uri },
        "position": { "line": 6, "character": 14 },
    }));
    let resp = client.wait_response(def_id, LONG);
    assert!(resp.get("error").is_none(), "definition returned an error: {resp:?}");
    let result = &resp["result"];
    assert_eq!(result["uri"], json!(uri), "definition should point back into the same file: {result:?}");
    assert_eq!(
        result["range"]["start"],
        json!({ "line": 1, "character": 6 }),
        "`def greet` name starts at 0-based line 1, col 6, got: {result:?}"
    );

    // A call site that cannot be resolved (no receiver class at all here)
    // must answer `null`, never a guess.
    let unknown_id = client.request("textDocument/definition", &json!({
        "textDocument": { "uri": uri },
        "position": { "line": 2, "character": 4 },
    }));
    let resp = client.wait_response(unknown_id, LONG);
    assert!(resp.get("error").is_none(), "definition returned an error: {resp:?}");
    assert_eq!(resp["result"], Value::Null, "no call site here: expected null, got: {resp:?}");

    let shutdown_id = client.request("shutdown", &Value::Null);
    client.wait_response(shutdown_id, LONG);
    client.notify("exit", &Value::Null);
    client.wait_exit(Duration::from_secs(5));
}
