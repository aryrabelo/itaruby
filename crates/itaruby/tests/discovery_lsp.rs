//! End-to-end LSP tests for declaration-source discovery (bead ita-16g):
//! `ita server` now runs the exact same `db/schema.rb`/`db/structure.sql`/
//! `sorbet/rbi` search `ita check` always ran
//! (`itaruby_semantic::wire_declaration_sources`), wired once at
//! `initialize`, before it existed at all. Proves both sides of the
//! contract: a project WITH a discoverable `sorbet/rbi` resolves a
//! constant only that RBI declares (E0104 silence) and announces the find
//! via `window/logMessage`; the exact same source text in a project
//! WITHOUT that `sorbet/rbi` still warns E0104 — the "before this bead"
//! control, so the first half proves something.
//!
//! Same minimal hand-rolled JSON-RPC framing as `containment_lsp.rs`: spawn
//! `ita server`, speak raw JSON-RPC over its stdio, prove what an editor
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

    /// Collects every message that arrives within `duration` — used here to
    /// gather the `window/logMessage`s discovery sends right after
    /// `initialize`, before any `didOpen` is even notified.
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

/// The constant `DiscoTk::Widget` only exists in a toy `sorbet/rbi/gems/`
/// fixture the server must discover upward from its workspace root
/// (`itaruby_semantic::wire_declaration_sources`, wired at `initialize`,
/// before `ProjectFiles`). Resolving it silences the E0104 the "before this
/// bead" control (below) still gets, and `initialize` must announce the
/// find via `window/logMessage` (Info) naming the discovered `sorbet/rbi`
/// path.
#[test]
fn discovered_sorbet_rbi_resolves_constant_and_silences_e0104_over_lsp() {
    let root_dir =
        std::env::temp_dir().join(format!("itaruby-discovery-lsp-with-rbi-{}", std::process::id()));
    std::fs::create_dir_all(&root_dir).unwrap();
    let _guard = TempDirGuard(root_dir.clone());

    std::fs::create_dir_all(root_dir.join("sorbet/rbi/gems")).unwrap();
    std::fs::write(
        root_dir.join("sorbet/rbi/gems/discotk.rbi"),
        "# typed: true\n\nclass DiscoTk::Widget\nend\n",
    )
    .unwrap();

    let mut client = LspClient::spawn();
    init_and_open_root(&mut client, &root_dir);

    // The discovery log fires unconditionally right after `initialize`,
    // before any `didOpen` — no request needed to observe it.
    let logs = client.drain_for(Duration::from_millis(500));
    let discovery_log = logs.iter().find(|m| {
        m.get("method").and_then(Value::as_str) == Some("window/logMessage")
            && m["params"]["message"]
                .as_str()
                .is_some_and(|s| s.contains("sorbet/rbi"))
    });
    assert!(
        discovery_log.is_some(),
        "expected a window/logMessage naming the discovered sorbet/rbi, got: {logs:?}"
    );

    let probe_path = root_dir.join("probe.rb");
    let probe_text = "class DiscoConsumer\n  def build\n    DiscoTk::Widget.new\n  end\nend\n";
    let probe_uri = file_uri(&probe_path);
    client.notify("textDocument/didOpen", &json!({
        "textDocument": { "uri": probe_uri, "languageId": "ruby", "version": 1, "text": probe_text }
    }));

    let diags = client.wait_diagnostics(&probe_uri, LONG, |_| true);
    assert!(
        diags.is_empty(),
        "DiscoTk::Widget is declared in the discovered sorbet/rbi: E0104 must be silent, got: {diags:?}"
    );

    let shutdown_id = client.request("shutdown", &Value::Null);
    client.wait_response(shutdown_id, LONG);
    client.notify("exit", &Value::Null);
    client.wait_exit(Duration::from_secs(5));
}

/// Control for the test above: the exact same source text, in a project
/// with no `sorbet/rbi` anywhere upward, must still warn E0104 — proving
/// the first test's silence comes from discovery, not from some unrelated
/// change to constant resolution. Also proves the "nothing found" shape of
/// the discovery log.
#[test]
fn without_sorbet_rbi_the_same_constant_still_warns_e0104_over_lsp() {
    let root_dir = std::env::temp_dir()
        .join(format!("itaruby-discovery-lsp-without-rbi-{}", std::process::id()));
    std::fs::create_dir_all(&root_dir).unwrap();
    let _guard = TempDirGuard(root_dir.clone());

    let mut client = LspClient::spawn();
    init_and_open_root(&mut client, &root_dir);

    let logs = client.drain_for(Duration::from_millis(500));
    let no_sources_log = logs.iter().find(|m| {
        m.get("method").and_then(Value::as_str) == Some("window/logMessage")
            && m["params"]["message"]
                .as_str()
                .is_some_and(|s| s.contains("no declaration sources found"))
    });
    assert!(
        no_sources_log.is_some(),
        "expected a window/logMessage reporting no declaration sources, got: {logs:?}"
    );

    let probe_path = root_dir.join("probe.rb");
    let probe_text = "class DiscoConsumer\n  def build\n    DiscoTk::Widget.new\n  end\nend\n";
    let probe_uri = file_uri(&probe_path);
    client.notify("textDocument/didOpen", &json!({
        "textDocument": { "uri": probe_uri, "languageId": "ruby", "version": 1, "text": probe_text }
    }));

    let diags = client.wait_diagnostics(&probe_uri, LONG, |d| !d.is_empty());
    assert_eq!(diags.len(), 1, "no sorbet/rbi discovered: E0104 must still fire, got: {diags:?}");
    assert_eq!(diags[0].get("code").and_then(Value::as_str), Some("E0104"), "got: {diags:?}");

    let shutdown_id = client.request("shutdown", &Value::Null);
    client.wait_response(shutdown_id, LONG);
    client.notify("exit", &Value::Null);
    client.wait_exit(Duration::from_secs(5));
}
