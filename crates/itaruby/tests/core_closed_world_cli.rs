//! End-to-end CLI tests for the closed-world conclusive core lookup (bead
//! ita-2ve): real binary, real discovery. The library tests
//! (`itaruby_semantic/tests/core_conclusive.rs`) wire `ClosedWorld`
//! explicitly because gem discovery lives in the BINARY
//! (`main.rs::gems_detected`) — only these tests prove the full contract:
//! typo fires in a project with no gems upward, goes silent the moment a
//! Gemfile/Gemfile.lock/*.gemspec sits anywhere upward of the checked
//! root, and the checked-in `with_gemfile/` fixture behaves per-root.
//!
//! Scratch trees go under cargo's own `CARGO_TARGET_TMPDIR` —
//! machine-local, wiped with `cargo clean`, never `/tmp` or `$HOME`.

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
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, name: &str, text: &str) {
    std::fs::write(dir.join(name), text).unwrap();
}

/// The p5 probe from the A/B artifact (verbatim body), planted in a
/// gemless tempdir: the narrowing typo fires E0101 naming String.
#[test]
fn p5_narrowing_probe_fires_without_gemfile() {
    let dir = tmpdir("core-closed-p5");
    write(
        &dir,
        "p5_narrowing_bug.rb",
        "# typed: true\nclass Appender\n  def append(x)\n    if x.is_a?(String)\n      x.pushh(1)\n    end\n    x\n  end\nend\n\nAppender.new.append(\"a\")\n",
    );
    let out = run_check(&dir);
    let text = stdout(&out);
    assert!(
        text.contains("E0101") && text.contains("pushh") && text.contains("String"),
        "p5 must fire E0101 for pushh on String, got: {text}"
    );
    assert_eq!(out.status.code(), Some(1), "planted error must exit 1");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The p3 probe (verbatim body): the ivar typo fires E0101 naming String.
#[test]
fn p3_ivar_probe_fires_without_gemfile() {
    let dir = tmpdir("core-closed-p3");
    write(
        &dir,
        "p3_ivar_type.rb",
        "# typed: true\nclass Label\n  def initialize\n    @name = \"hello\"\n  end\n\n  def describe\n    @name\n  end\n\n  def mutate\n    @name.push(1)\n  end\nend\n\nlabel = Label.new\nlabel.describe\n",
    );
    let out = run_check(&dir);
    let text = stdout(&out);
    assert!(
        text.contains("E0101") && text.contains("push") && text.contains("String"),
        "p3 must fire E0101 for push on String, got: {text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Same probes, one Gemfile beside them: silence. A gem can monkeypatch
/// core in ways no static inventory sees, so the conclusive path is off.
#[test]
fn same_probes_go_silent_with_gemfile() {
    let dir = tmpdir("core-closed-gemfile");
    write(
        &dir,
        "p5_narrowing_bug.rb",
        "# typed: true\nclass Appender\n  def append(x)\n    if x.is_a?(String)\n      x.pushh(1)\n    end\n    x\n  end\nend\n\nAppender.new.append(\"a\")\n",
    );
    write(
        &dir,
        "p3_ivar_type.rb",
        "# typed: true\nclass Label\n  def initialize\n    @name = \"hello\"\n  end\n\n  def describe\n    @name\n  end\n\n  def mutate\n    @name.push(1)\n  end\nend\n\nlabel = Label.new\nlabel.describe\n",
    );
    write(&dir, "Gemfile", "source \"https://rubygems.org\"\n");
    let out = run_check(&dir);
    let text = stdout(&out);
    assert!(
        !text.contains("E0101"),
        "Gemfile upward of the root must disable the conclusive path, got: {text}"
    );
    assert_eq!(out.status.code(), Some(0), "no errors expected, got: {text}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A `Gemfile.lock` alone is enough to open the world; a `*.gemspec` is
/// too — one probe each.
#[test]
fn gemfile_lock_and_gemspec_also_open_the_world() {
    for (name, gem_file, content) in [
        ("lock", "Gemfile.lock", "GEM\n  remote: https://rubygems.org/\n"),
        ("gemspec", "itaruby_probe.gemspec", "Gem::Specification.new do |s|\nend\n"),
    ] {
        let dir = tmpdir(&format!("core-closed-{name}"));
        write(
            &dir,
            "p5_narrowing_bug.rb",
            "# typed: true\nclass Appender\n  def append(x)\n    if x.is_a?(String)\n      x.pushh(1)\n    end\n    x\n  end\nend\n\nAppender.new.append(\"a\")\n",
        );
        write(&dir, gem_file, content);
        let out = run_check(&dir);
        let text = stdout(&out);
        assert!(
            !text.contains("E0101"),
            "{gem_file} upward of the root must disable the conclusive path, got: {text}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The checked-in `with_gemfile/` fixture: silent when checked as its own
/// root (Gemfile discovered upward), and the Gemfile nested below
/// `testdata/` does NOT open the world for the sibling fixtures — per-root
/// discovery only walks upward. Both directions asserted in one run over
/// the fixture dir and one over the parent `core_conclusive/` dir.
#[test]
fn with_gemfile_fixture_is_per_root() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/core_conclusive");

    // Checked as its own root: Gemfile sits right there -> silence.
    let out = run_check(&fixture.join("with_gemfile"));
    let text = stdout(&out);
    assert!(
        !text.contains("E0101"),
        "with_gemfile checked as its own root must be silent, got: {text}"
    );

    // One level up (no gems upward of core_conclusive/): the gated typo
    // fires like any other, the real-method and reopened fixtures stay
    // silent, and no E0101 leaks out of files that must be quiet.
    let out = run_check(&fixture);
    let text = stdout(&out);
    assert!(
        text.contains("narrowing_core_typo.rb") && text.contains("E0101") && text.contains("pushh"),
        "typo fixtures must fire under core_conclusive/, got: {text}"
    );
    assert!(
        text.contains("ivar_core_typo.rb") && text.contains("push"),
        "ivar typo must fire too, got: {text}"
    );
    for quiet in ["core_real_method_silent.rb", "reopened_core_silent.rb"] {
        assert!(
            !text.contains(quiet),
            "{quiet} must stay silent in the merged run, got: {text}"
        );
    }
}
