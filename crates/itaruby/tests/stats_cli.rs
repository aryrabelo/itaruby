//! End-to-end CLI tests for the `ita check --stats` unknown-receiver
//! breakdown (bead ita-au5): real binary, real discovery. This bead is a
//! MEASUREMENT bead only — it must never change a diagnostic — so the
//! contract under test is entirely about the `--stats` census output: the
//! 8 new sub-bucket labels appear, they sum exactly to `receiver unknown`
//! (the invariant the CLI must never silently break), and the diagnostic
//! stream + exit code are byte-for-byte unaffected by the flag. Same
//! tempdir pattern as `core_closed_world_cli.rs` / `agent_format.rs`:
//! scratch trees live under cargo's own `CARGO_TARGET_TMPDIR`, never
//! `/tmp`, and carry no `Gemfile` above them.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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

/// Four blind call sites, each with a distinct `Ty::Unknown` origin, plus
/// one genuinely diagnosable call (the `ivar_core_typo.rb` shape reused
/// verbatim from `agent_format.rs`/`core_closed_world_cli.rs`) so the
/// exit-code and diagnostic-stream comparisons below are non-trivial —
/// exit 1, one real E0101 line — rather than a vacuous "both empty".
const FIXTURE: &str = "\
class Widget
  # receiver is an unrefined method parameter
  def use_param(x)
    x.frobnicate
  end

  # chain died in a core method with no modeled return
  def use_chain
    \"hello\".encode.frobnicate
  end

  # receiver is an ivar that collapsed to Unknown (no visible assignment)
  def use_ivar
    @thing.frobnicate
  end

  # receiver is an unresolved constant
  def use_const
    UndefinedConst.frobnicate
  end
end

class ConstrTypo
  def initialize
    @tag = \"hello\"
  end

  def mutate
    @tag.push(1)
  end
end
";

/// Pulls the leading count off a `--stats` line whose trimmed text starts
/// with `label` — e.g. `label = \"method param\"` matches
/// `      method param   {count}  {pct}%  (...)`. Panics with the full
/// stdout on a miss so a failure names exactly which label went missing.
fn parse_count(text: &str, label: &str) -> u64 {
    let line = text
        .lines()
        .find(|l| l.trim_start().starts_with(label))
        .unwrap_or_else(|| panic!("no stats line starting with {label:?} in:\n{text}"));
    let rest = line.trim_start().strip_prefix(label).unwrap();
    let count = rest
        .split_whitespace()
        .next()
        .unwrap_or_else(|| panic!("no count token after {label:?} in line: {line:?}"));
    count
        .parse()
        .unwrap_or_else(|_| panic!("count token {count:?} after {label:?} isn't a u64: {line:?}"))
}

/// Every one of the 8 sub-bucket labels from the batch contract renders,
/// and the 8 counts sum exactly to `receiver unknown` — the invariant the
/// CLI must never silently break, since the sub-buckets are a breakdown
/// of that bucket, never additional call sites (`CallStats::total`/
/// `blind` stay unchanged).
#[test]
fn rbi_method_bucket_renders() {
    let dir = tmpdir("stats-cli-rbi-method");
    write(&dir, "widget.rb", FIXTURE);
    let out = run_check(&dir, &["--stats"]);
    let text = stdout(&out);

    assert!(
        text.contains("rbi method"),
        "missing rbi method bucket in:\n{text}"
    );
    // No RBIs are wired into this fixture, so the bucket is well-formed
    // but empty; `parse_count` panics if the line is malformed.
    let rbi_method = parse_count(&text, "rbi method");
    assert_eq!(
        rbi_method, 0,
        "expected no rbi-resolved calls in this fixture, got {rbi_method} in:\n{text}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Same shape as `rbi_method_bucket_renders`: the DSL RBI census bucket
/// (bead ita-tjr) renders as its own line right after `rbi method`, well
/// formed even when no DSL RBIs are wired into the fixture.
#[test]
fn dsl_method_bucket_renders() {
    let dir = tmpdir("stats-cli-dsl-method");
    write(&dir, "widget.rb", FIXTURE);
    let out = run_check(&dir, &["--stats"]);
    let text = stdout(&out);

    assert!(
        text.contains("dsl method"),
        "missing dsl method bucket in:\n{text}"
    );
    // No RBIs are wired into this fixture, so the bucket is well-formed
    // but empty; `parse_count` panics if the line is malformed.
    let dsl_method = parse_count(&text, "dsl method");
    assert_eq!(
        dsl_method, 0,
        "expected no dsl-resolved calls in this fixture, got {dsl_method} in:\n{text}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unknown_receiver_sub_buckets_sum_to_the_parent_bucket() {
    let dir = tmpdir("stats-cli-sub-buckets");
    write(&dir, "widget.rb", FIXTURE);
    let out = run_check(&dir, &["--stats"]);
    let text = stdout(&out);

    for label in [
        "method param",
        "block param",
        "core ret",
        "project ret",
        "dead chain",
        "ivar",
        "constant",
        "other",
    ] {
        assert!(
            text.contains(label),
            "missing sub-bucket label {label:?} in:\n{text}"
        );
    }

    let receiver_unknown = parse_count(&text, "receiver unknown");
    let sum = parse_count(&text, "method param")
        + parse_count(&text, "block param")
        + parse_count(&text, "core ret")
        + parse_count(&text, "project ret")
        + parse_count(&text, "dead chain")
        + parse_count(&text, "ivar")
        + parse_count(&text, "constant")
        + parse_count(&text, "other");
    assert_eq!(
        sum, receiver_unknown,
        "8 sub-buckets must sum exactly to `receiver unknown`; got sum={sum} vs receiver_unknown={receiver_unknown} in:\n{text}"
    );
    // The fixture plants 4 distinct unknown-receiver call sites
    // (param/chain/ivar/const), so the bucket must be non-trivial —
    // otherwise the sum-equals-zero case would pass this test vacuously.
    assert!(
        receiver_unknown >= 4,
        "expected at least 4 blind call sites, got {receiver_unknown} in:\n{text}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// `--stats` is purely additive: the diagnostic stream (everything before
/// the appended `\ncoverage:` census block) and the exit code are
/// byte-for-byte identical with and without the flag.
#[test]
fn stats_flag_never_changes_diagnostics_or_exit_code() {
    let dir = tmpdir("stats-cli-no-regression");
    write(&dir, "widget.rb", FIXTURE);

    let without = run_check(&dir, &[]);
    let with = run_check(&dir, &["--stats"]);
    let without_text = stdout(&without);
    let with_text = stdout(&with);

    assert_eq!(
        without.status.code(),
        with.status.code(),
        "exit code must not depend on --stats"
    );
    // The planted E0101 (`@tag.push(1)`, String has no `push`) must fire
    // either way — this is not a vacuous "both empty" comparison.
    assert!(
        without_text.contains("E0101"),
        "fixture must still diagnose without --stats: {without_text}"
    );

    assert!(
        with_text.starts_with(&without_text),
        "the --stats run's diagnostic prefix must be byte-identical to the plain run;\nwithout:\n{without_text}\nwith:\n{with_text}"
    );
    let appended = &with_text[without_text.len()..];
    assert!(
        appended.starts_with("\ncoverage:"),
        "--stats must only ever APPEND the coverage census after the identical diagnostic stream, got appended:\n{appended}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
