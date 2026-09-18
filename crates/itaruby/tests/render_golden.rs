//! Golden rendering tests (w12 closure): the real binary against real
//! fixtures in `tests/golden/`, pinning the CLI rendering contract the
//! blind verdict asked for —
//!
//!   * E0101/E0104 carry `did you mean` exactly when a known name is
//!     within edit distance 2 (absent otherwise),
//!   * E0101's suggestion carries `defined at path:line` when the index
//!     knows the def site; E0104's carries it exactly when the winner is
//!     a VALUE constant the index has a site for (w12 closure) — class/
//!     module-path winners still carry none, never invented,
//!   * no phase-log line ever appears unless `--verbose` is passed.
//!
//! Fixtures live here rather than under `testdata/` on purpose: gate c
//! hashes `ita check testdata/` output and must not gain a single
//! diagnostic from a rendering bead.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn golden(p: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(p)
}

fn run_check(paths: &[PathBuf], verbose: bool) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ita"));
    cmd.arg("check");
    if verbose {
        cmd.arg("--verbose");
    }
    for p in paths {
        cmd.arg(p);
    }
    cmd.output().expect("run `ita check`")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn method_typo_renders_excerpt_did_you_mean_and_defined_at() {
    let out = run_check(&[golden("calc.rb"), golden("typo_call.rb")], false);
    let s = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "one error => exit 1");
    assert!(s.contains("error[E0101]"), "got:\n{s}");
    // Excerpt: the offending source line, numbered, then the caret line.
    assert!(s.contains(" | Calculator.new.calculat(1)"), "excerpt line missing:\n{s}");
    // `calculat` is 8 chars: the caret span matches the symbol, not 1 caret.
    assert!(s.contains("^^^^^^^^ did you mean `calculate`?"), "caret+dym missing:\n{s}");
    // Defined-here names the DEFINING file and line (cross-file proof).
    assert!(
        s.lines().any(|l| l.contains("defined at") && l.contains("calc.rb:2")),
        "defined-at must point at calc.rb line 2 (the `def calculate`), got:\n{s}"
    );
}

#[test]
fn method_typo_beyond_distance_two_gets_no_did_you_mean() {
    let out = run_check(&[golden("far_typo.rb")], false);
    let s = stdout(&out);
    assert!(s.contains("error[E0101]"), "got:\n{s}");
    assert!(!s.contains("did you mean"), "no candidate within distance 2:\n{s}");
    assert!(s.contains("^^^^^^^^^"), "excerpt/caret still rendered:\n{s}");
}

#[test]
fn constant_typo_gets_did_you_mean_and_class_winners_never_define_at() {
    let out = run_check(&[golden("const_typo.rb")], false);
    let s = stdout(&out);
    assert!(s.contains("warning[E0104]"), "got:\n{s}");
    assert!(
        s.contains("did you mean `Invoice`?"),
        "Invoicee is distance 1 from Invoice:\n{s}"
    );
    assert!(
        !s.contains("defined at"),
        "`Invoice` is a class/module path — the index tracks def sites only\
         \nfor value constants, omitted never invented:\n{s}"
    );
}

#[test]
fn phase_logs_stay_off_the_output_unless_verbose() {
    // Default: stdout is diagnostics only, stderr is empty — no phase noise.
    let out = run_check(&[golden("typo_call.rb"), golden("const_typo.rb")], false);
    assert!(!stdout(&out).contains("rbi phase"), "stdout leaked a phase log");
    assert!(stderr(&out).is_empty(), "stderr must be clean without --verbose, got: {}", stderr(&out));

    // --verbose is the only place phase logs appear (stderr), documented
    // in the usage text.
    let out = run_check(&[golden("typo_call.rb"), golden("const_typo.rb")], true);
    assert!(!stdout(&out).contains("rbi phase"), "phase logs never go to stdout");
    assert!(stderr(&out).contains("rbi phase 2"), "verbose must show the phase log on stderr");
}

#[test]
fn usage_documents_the_verbose_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_ita")).output().expect("run `ita`");
    let usage = stderr(&out);
    assert!(usage.contains("--verbose"), "usage must document --verbose:\n{usage}");
}
