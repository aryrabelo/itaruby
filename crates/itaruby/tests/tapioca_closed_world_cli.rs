//! End-to-end CLI tests for closed-world-via-Tapioca (w12 closure). The
//! old bead ita-2ve contract — any Gemfile upward opens the world — is
//! generalized: a Gemfile WITH complete Tapioca coverage (every gem in
//! `Gemfile.lock`'s GEM/specs has a `sorbet/rbi/gems/<name>@*.rbi`) keeps
//! the world CLOSED, because the core monkeypatches gems make are exactly
//! what those RBIs declare; the conclusive lookup consults them (condition
//! (d) in `check.rs`). Any coverage gap fails closed to the old silence.
//!
//! These live as CLI tempdir tests (not `testdata/` fixtures) because
//! discovery is per-root and `testdata/` has no Gemfile — see
//! `core_closed_world_cli.rs` for the gemless side of the same contract.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn run_check(arg: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ita"))
        .arg("check")
        .arg(arg)
        .output()
        .expect("run `ita check`")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn tmpdir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sorbet/rbi/gems")).unwrap();
    dir
}

fn write(dir: &Path, name: &str, text: &str) {
    std::fs::write(dir.join(name), text).unwrap();
}

/// The p5 narrowing probe plus a real ActiveSupport-style `String#squish`
/// call: with one gem fully covered, the typo still fires E0101 while the
/// RBI-declared method stays silent — declared means exists.
#[test]
fn full_coverage_closes_the_world_and_rbi_declared_methods_stay_silent() {
    let dir = tmpdir("tapioca-closed-full");
    write(&dir, "Gemfile", "source \"https://rubygems.org\"\ngem \"mygem\"\n");
    write(&dir, "Gemfile.lock", "GEM\n  remote: https://rubygems.org/\n  specs:\n    mygem (1.0.0)\n");
    write(
        &dir.join("sorbet/rbi/gems"),
        "mygem@1.0.0.rbi",
        "# typed: true\n\nclass String\n  def squish; end\nend\n",
    );
    write(
        &dir,
        "probe.rb",
        "# typed: true\nclass Appender\n  def append(x)\n    if x.is_a?(String)\n      x.pushh(1)\n    end\n    x\n  end\nend\n\nAppender.new.append(\"a\")\n\"s\".squish\n",
    );
    let out = run_check(&dir);
    let text = stdout(&out);
    assert!(
        text.contains("E0101") && text.contains("pushh"),
        "covered Tapioca project: typo must fire E0101, got: {text}"
    );
    assert!(
        !text.contains("squish"),
        "String#squish is declared in the gem RBI — silence, got: {text}"
    );
    assert_eq!(out.status.code(), Some(1), "one planted error => exit 1");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Same project, RBI file deleted: coverage incomplete, mode off, total
/// silence — exactly the pre-w12 behavior for any Gemfile project.
#[test]
fn missing_gem_rbi_fails_closed_to_silence() {
    let dir = tmpdir("tapioca-closed-missing");
    write(&dir, "Gemfile", "source \"https://rubygems.org\"\ngem \"mygem\"\n");
    write(&dir, "Gemfile.lock", "GEM\n  remote: https://rubygems.org/\n  specs:\n    mygem (1.0.0)\n");
    write(
        &dir,
        "probe.rb",
        "# typed: true\nclass Appender\n  def append(x)\n    if x.is_a?(String)\n      x.pushh(1)\n    end\n    x\n  end\nend\n\nAppender.new.append(\"a\")\n",
    );
    let out = run_check(&dir);
    let text = stdout(&out);
    assert!(
        !text.contains("E0101"),
        "one gem without its RBI opens the world: silence, got: {text}"
    );
    assert_eq!(out.status.code(), Some(0), "no errors expected, got: {text}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Lock names two gems, only one RBI exists: incomplete coverage, mode
/// off — the gap gem could patch core invisibly.
#[test]
fn two_gems_one_rbi_is_incomplete_coverage() {
    let dir = tmpdir("tapioca-closed-partial");
    write(&dir, "Gemfile", "source \"https://rubygems.org\"\ngem \"mygem\"\ngem \"other\"\n");
    write(
        &dir,
        "Gemfile.lock",
        "GEM\n  remote: https://rubygems.org/\n  specs:\n    mygem (1.0.0)\n    other (2.0.0)\n",
    );
    write(
        &dir.join("sorbet/rbi/gems"),
        "mygem@1.0.0.rbi",
        "# typed: true\n\nclass String\n  def squish; end\nend\n",
    );
    write(
        &dir,
        "probe.rb",
        "# typed: true\nclass Appender\n  def append(x)\n    if x.is_a?(String)\n      x.pushh(1)\n    end\n    x\n  end\nend\n\nAppender.new.append(\"a\")\n",
    );
    let out = run_check(&dir);
    let text = stdout(&out);
    assert!(
        !text.contains("E0101"),
        "2 gems in lock, 1 RBI: coverage incomplete, silence, got: {text}"
    );
    assert_eq!(out.status.code(), Some(0), "no errors expected, got: {text}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// w12 closure, E0104 did-you-mean for VALUE constants: `Foo::Barr`
/// suggests `Foo::Bar` — the indexed `ConstantWriteNode` — with a real
/// `defined at` pointing at the assignment. Golden rendering, gemless
/// tempdir (E0104 is not closed-world-gated).
#[test]
fn value_constant_typo_gets_did_you_mean_and_defined_at() {
    let dir = tmpdir("tapioca-const-golden");
    write(
        &dir,
        "foo.rb",
        "module Foo\n  Bar = 1\nend\n\np Foo::Barr\n",
    );
    let out = run_check(&dir);
    let text = stdout(&out);
    assert!(text.contains("warning[E0104]"), "got:\n{text}");
    assert!(
        text.contains("did you mean `Foo::Bar`?"),
        "qualified typo suggests the indexed value constant:\n{text}"
    );
    assert!(
        text.lines().any(|l| l.contains("defined at") && l.contains("foo.rb:2")),
        "defined-at must point at foo.rb line 2 (the `Bar = 1`), got:\n{text}"
    );
    assert_eq!(out.status.code(), Some(0), "warning only => exit 0");
    let _ = std::fs::remove_dir_all(&dir);
}
