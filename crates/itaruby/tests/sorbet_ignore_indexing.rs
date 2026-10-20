//! H4 (2026-10-20): `sorbet/config` `--ignore` means "don't TYPECHECK
//! these files", never "they don't exist at runtime". An ignored file is
//! read as SUPPRESSION-ONLY evidence (`itaruby_semantic::IgnoredFiles`):
//! its definitions silence E0104 elsewhere and open what they reopen, but
//! never close a name, and nothing inside it is ever reported.
//!
//! Fixture: `testdata/sorbet_ignore_indexing/` (read-only, checked in
//! place). Every assertion names the EXACT historical diagnostic so a
//! mutant in `scripts/ignore-indexing-mutants.sh` re-emits it verbatim.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/sorbet_ignore_indexing")
        .canonicalize()
        .expect("fixture tree exists")
}

fn check() -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_ita"))
        .arg("check")
        .arg(fixture())
        .output()
        .expect("run `ita check`");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A constant defined ONLY in an ignored initializer, used from checked
/// code: real at runtime, so no E0104 (planningalerts'
/// `ElasticSearchClient = if ... end` shape).
#[test]
fn ignored_only_constant_resolves_without_e0104() {
    let out = check();
    assert!(
        !out.contains("unresolved constant `SearchClient`"),
        "a constant defined only under an ignored path must not read as unresolved:\n{out}"
    );
}

/// The trap: a closed test-fixture stub named like a gem class (a gem
/// with no RBI here, so nothing else declares it) must never become the
/// class that name resolves to — `Faraday.get` is a real gem method and
/// the stub only defines `stub_only`. The stub may silence the E0104 (the
/// name exists at runtime), never accuse.
#[test]
fn ignored_fixture_stub_never_closes_a_gem_class() {
    let out = check();
    assert!(
        !out.contains("undefined method `get` for class `Faraday`"),
        "an ignored fixture stub must not close the gem class it shadows:\n{out}"
    );
    assert!(
        !out.contains("unresolved constant `Faraday`"),
        "the stub is runtime evidence that `Faraday` exists:\n{out}"
    );
}

/// An ignored initializer that reopens a project class is real at
/// runtime: the class it reopens cannot conclude a miss on the method the
/// initializer adds.
#[test]
fn ignored_reopening_opens_the_project_class() {
    let out = check();
    assert!(
        !out.contains("undefined method `from_initializer` for `Widget`"),
        "a project class reopened by an ignored file must be open:\n{out}"
    );
}

/// Nothing is ever reported INSIDE an ignored path: neither the E0104 a
/// checked fixture would carry, nor anything from the intentionally
/// broken one.
#[test]
fn nothing_is_reported_inside_an_ignored_path() {
    let out = check();
    assert!(
        !out.contains("unresolved constant `UndefinedInsideIgnoredFixture`"),
        "a diagnostic inside an ignored file must not be reported:\n{out}"
    );
    assert!(
        !out.contains("spec/fixtures/") && !out.contains("config/initializers/"),
        "no line may point inside an ignored path (broken Ruby included):\n{out}"
    );
}

/// Control: a constant defined nowhere — not in checked code, not under
/// an ignored path — still gets its E0104.
#[test]
fn typo_constant_defined_nowhere_still_warns() {
    let out = check();
    assert!(
        out.contains("app/typo.rb:3:5: warning[E0104]: unresolved constant `TypoConstantDefinedNowhere`"),
        "a constant defined nowhere must still warn:\n{out}"
    );
}

/// Control: a true E0101 outside every ignored path still fires.
#[test]
fn true_e0101_outside_ignored_paths_still_fires() {
    let out = check();
    assert!(
        out.contains("app/gadget.rb:6:12: error[E0101]: undefined method `reel` for `Gadget`"),
        "a real typo on a closed project class must still accuse:\n{out}"
    );
}

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

fn shutdown(client: &mut LspClient) {
    let shutdown_id = client.request("shutdown", &Value::Null);
    client.wait_response(shutdown_id, LONG);
    client.notify("exit", &Value::Null);
    client.wait_exit(Duration::from_secs(5));
}

fn pull(client: &mut LspClient, path: &std::path::Path) -> Vec<Value> {
    let uri = file_uri(path);
    let text = std::fs::read_to_string(path).unwrap();
    client.notify("textDocument/didOpen", &json!({
        "textDocument": { "uri": uri, "languageId": "ruby", "version": 1, "text": text }
    }));
    let id = client.request("textDocument/diagnostic", &json!({ "textDocument": { "uri": uri } }));
    let resp = client.wait_response(id, LONG);
    assert!(resp.get("error").is_none(), "pull returned an error: {resp:?}");
    resp["result"]["items"].as_array().cloned().unwrap_or_default()
}

/// The LSP honors `sorbet/config` exactly like `ita check`: the ignored
/// initializer's constant silences E0104 in checked code, an ignored file
/// opened in the editor gets no diagnostics, and the typo control still
/// warns.
#[test]
fn lsp_honors_ignore_like_the_cli() {
    let root = fixture();
    let mut client = LspClient::spawn();
    init_and_open_root(&mut client, &root);

    let indexer = pull(&mut client, &root.join("app/search_indexer.rb"));
    assert!(indexer.is_empty(), "ignored-only constant must not warn in the LSP: {indexer:?}");
    let inside = pull(&mut client, &root.join("spec/fixtures/typo_inside.rb"));
    assert!(inside.is_empty(), "an ignored file must get no diagnostics in the LSP: {inside:?}");
    let http = pull(&mut client, &root.join("app/http.rb"));
    assert!(http.is_empty(), "an ignored stub must not close the gem class in the LSP: {http:?}");
    let typo = pull(&mut client, &root.join("app/typo.rb"));
    assert!(
        typo.iter().any(|d| d["code"] == json!("E0104")),
        "control: a constant defined nowhere still warns in the LSP: {typo:?}"
    );

    shutdown(&mut client);
}
