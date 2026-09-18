//! End-to-end CLI tests for `ita check --format=json` (prototype C, delivery
//! parsing/render): the JSONL diagnostic channel that feeds the ruby-lsp
//! addon bridge. Same tempdir pattern as `agent_format.rs`/
//! `core_closed_world_cli.rs`: scratch trees live under cargo's own
//! `CARGO_TARGET_TMPDIR`, never `/tmp`, and carry no `Gemfile` above them.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn run_check(dir: &Path, extra_args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ita"))
        .arg("check")
        .args(extra_args)
        .arg(dir)
        .output()
        .expect("run `ita check`")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn tmpdir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, name: &str, text: &str) {
    std::fs::write(dir.join(name), text).unwrap();
}

/// A single, unambiguous E0101: `@tag` typed `String` by `initialize`, then
/// a `push` call — `push` isn't a `String` method. Reused verbatim from
/// `core_conclusive/ivar_core_typo.rb`'s shape.
const IVAR_TYPO: &str = "\
class ConstrTypo
  def initialize
    @tag = \"hello\"
  end

  def mutate
    @tag.push(1)
  end
end
";

const CLEAN: &str = "\
class Clean
  def greet
    \"hi\".upcase
  end
end
";

/// (a) A project with one known error: stdout is valid JSONL (one JSON
/// object per line), the object carries the contract-2 fields with
/// 1-based `line`/`column`, and the exit code matches the normal channel's
/// (`error[E0101]` → exit 1).
#[test]
fn known_error_produces_parseable_jsonl_with_expected_fields() {
    let dir = tmpdir("json_known_error");
    write(&dir, "typo.rb", IVAR_TYPO);

    let plain = run_check(&dir, &[]);
    assert_eq!(plain.status.code(), Some(1), "sanity: normal run stdout:\n{}", stdout(&plain));

    let out = run_check(&dir, &["--format=json"]);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "exit code must match the normal run, stdout:\n{text}");

    let lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 1, "expected exactly one diagnostic line, got:\n{text}");

    let obj: Value = serde_json::from_str(lines[0]).expect("line must parse as JSON");
    assert!(
        obj["path"].as_str().unwrap().ends_with("typo.rb"),
        "unexpected path, got: {obj}"
    );
    assert_eq!(obj["line"], 7, "got: {obj}");
    assert_eq!(obj["column"], 10, "got: {obj}");
    assert_eq!(obj["code"], "E0101", "got: {obj}");
    assert_eq!(obj["severity"], "error", "got: {obj}");
    assert_eq!(
        obj["message"], "undefined method `push` for `String`",
        "got: {obj}"
    );
    // No excerpt/caret/did-you-mean payload belongs in this rendering yet.
    assert!(obj.get("suggestions").is_none(), "got: {obj}");
}

/// (a2) E0108 (operand type mismatch) travels the same JSONL channel
/// with no per-code wiring: same fields, 1-based position on the OPERAND,
/// `severity: "error"`, and the exit code the human channel gives. The
/// snippet is the capability's reference case and really raises
/// `TypeError: String can't be coerced into Integer` under MRI.
#[test]
fn operand_type_mismatch_travels_the_json_channel() {
    let dir = tmpdir("json_operand_types");
    write(
        &dir,
        "price.rb",
        "price = 100\nlabel = \"R$ #{price}\"\nprice + label\n",
    );

    let out = run_check(&dir, &["--format=json"]);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "stdout:\n{text}");

    let lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 1, "expected exactly one diagnostic, got:\n{text}");
    let obj: Value = serde_json::from_str(lines[0]).expect("line must parse as JSON");
    assert_eq!(obj["code"], "E0108", "got: {obj}");
    assert_eq!(obj["severity"], "error", "got: {obj}");
    assert_eq!(obj["line"], 3, "got: {obj}");
    assert_eq!(obj["column"], 9, "got: {obj}");
    assert_eq!(
        obj["message"], "`+` on Integer expects a numeric operand, got String",
        "got: {obj}"
    );
}

/// (b) A clean project under `--format=json`: no findings, empty stdout,
/// exit 0 — no clean-run footer (that's `--format=agent`'s contract, not
/// this channel's).
#[test]
fn clean_project_is_empty_stdout_and_exit_0() {
    let dir = tmpdir("json_clean");
    write(&dir, "clean.rb", CLEAN);

    let out = run_check(&dir, &["--format=json"]);
    assert_eq!(out.status.code(), Some(0), "stderr was:\n{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(stdout(&out), "", "expected empty stdout for a clean run under --format=json");
}

/// (c) An unrecognized `--format` value is still a usage error, exit 2 —
/// `json` joining `agent` as the only two valid values doesn't relax this.
#[test]
fn unknown_format_value_exits_2() {
    let dir = tmpdir("json_bogus_format");
    write(&dir, "typo.rb", IVAR_TYPO);

    let out = run_check(&dir, &["--format=bogus"]);
    assert_eq!(out.status.code(), Some(2), "stderr was:\n{}", String::from_utf8_lossy(&out.stderr));
}

/// (d) Non-regression: the SAME project, without `--format`, still prints
/// the pre-existing human report (caret excerpt included) and that output
/// does NOT parse as JSON — the two channels never bleed into each other.
#[test]
fn without_format_flag_output_is_unchanged_human_report() {
    let dir = tmpdir("json_non_regression");
    write(&dir, "typo.rb", IVAR_TYPO);

    let out = run_check(&dir, &[]);
    let text = stdout(&out);

    assert!(text.contains("error[E0101]: undefined method `push` for `String`"), "got:\n{text}");
    assert!(text.contains("@tag.push(1)"), "excerpt source line missing, got:\n{text}");
    assert!(text.contains('^'), "caret line missing, got:\n{text}");
    assert!(
        serde_json::from_str::<Value>(&text).is_err(),
        "human report must not parse as a single JSON value, got:\n{text}"
    );
    for line in text.lines().filter(|l| !l.is_empty()) {
        assert!(
            serde_json::from_str::<Value>(line).is_err(),
            "no line of the human report should parse as JSON on its own: {line:?}"
        );
    }
}
