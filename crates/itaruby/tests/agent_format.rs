//! End-to-end CLI tests for `ita check --format=agent` (bead ita-dqo,
//! entrega 2): the rendering-only agent channel over the E0107
//! constraint-contradiction diagnostic and the `Inferred`/`UnionCandidate`
//! `constraint_report` outcomes. Same tempdir pattern as
//! `core_closed_world_cli.rs`: scratch trees live under cargo's own
//! `CARGO_TARGET_TMPDIR`, never `/tmp`, and carry no `Gemfile` above them
//! so `ClosedWorld` is on — E0107 is impossible otherwise (see
//! `main.rs::wire_closed_world`).

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

/// The bead's own canonical contradiction shape (RUN-LOG turno 5): an
/// untyped parameter `x` calls `upcase` (a `String` method) then `push`
/// (an `Array` method) — no core class answers to both, so the two
/// candidate sets have an empty intersection.
const CONTRADICTION: &str = "\
class ConstrContradiction
  def process(x)
    x.upcase
    x.push(1)
  end
end
";

/// bead ita-2ve's proven core-conclusive shape (`ivar_core_typo.rb`),
/// reused verbatim: an ivar typed `String` by `initialize`, then a `push`
/// call on it — `push` isn't a `String` method, so it's a genuine E0101
/// Error. The receiver is an ivar, never a `LocalVariableReadNode`, so
/// this collects zero constraint calls — useful as the "constraints
/// don't touch unrelated diagnostics" control.
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

/// (a) A contradiction fixture under `--format=agent`: stdout carries the
/// `## constraint contradiction` block naming both call sites and their
/// candidates.
#[test]
fn contradiction_renders_agent_block_with_both_call_sites() {
    let dir = tmpdir("contradiction_agent_block");
    write(&dir, "contradiction.rb", CONTRADICTION);

    let out = run_check(&dir, &["--format=agent"]);
    let text = stdout(&out);

    assert!(
        text.contains("## constraint contradiction: `x`"),
        "missing contradiction heading, got:\n{text}"
    );
    assert!(text.contains("`upcase`"), "missing upcase call site, got:\n{text}");
    assert!(text.contains("`push`"), "missing push call site, got:\n{text}");
    assert!(text.contains("candidates:"), "missing candidates list, got:\n{text}");
    assert!(text.contains("**Resolution**"), "missing resolution section, got:\n{text}");
}

/// (b) Same fixture WITHOUT the flag: the normal `warning[E0107]` line
/// appears, and no markdown heading leaks into the default channel.
#[test]
fn contradiction_without_flag_prints_normal_warning_only() {
    let dir = tmpdir("contradiction_normal");
    write(&dir, "contradiction.rb", CONTRADICTION);

    let out = run_check(&dir, &[]);
    let text = stdout(&out);

    assert!(text.contains("warning[E0107]"), "missing E0107 warning line, got:\n{text}");
    assert!(!text.contains("##"), "markdown heading leaked into the normal channel:\n{text}");
}

/// (c) An unrecognized `--format` value is a usage error, exit 2 — same
/// contract as every other malformed flag this binary already rejects.
#[test]
fn unknown_format_value_exits_2() {
    let dir = tmpdir("bogus_format");
    write(&dir, "contradiction.rb", CONTRADICTION);

    let out = run_check(&dir, &["--format=bogus"]);
    assert_eq!(out.status.code(), Some(2), "stderr was:\n{}", String::from_utf8_lossy(&out.stderr));
}

/// (d) A/B non-regression on a fixture with no constraint calls at all
/// (an ivar receiver, never collected): without the flag, output is
/// byte-identical to the pre-existing E0101 report; with the flag, the
/// only output is the explicit clean-run footer — nothing about this
/// file's E0101 belongs on the dedicated constraint channel, and the
/// footer disambiguates "checked and clean" from "never ran" for an
/// unattended agent (verdict-constraints-r1 gap 1).
#[test]
fn file_without_constraints_is_unchanged_without_flag_and_empty_with_it() {
    let dir = tmpdir("no_constraints_ab");
    write(&dir, "typo.rb", IVAR_TYPO);

    let normal = stdout(&run_check(&dir, &[]));
    assert!(normal.contains("error[E0101]: undefined method `push` for `String`"), "got:\n{normal}");
    assert!(normal.contains("@tag.push(1)"), "excerpt line missing, got:\n{normal}");

    let agent = stdout(&run_check(&dir, &["--format=agent"]));
    assert_eq!(
        agent, "_Checked 1 file(s); 0 constraint finding(s)._\n",
        "expected only the summary footer for a file with no constraint calls"
    );
    assert!(!agent.contains("##"), "markdown block leaked into a clean run:\n{agent}");
}

/// (e) Exit code is the same regardless of `--format`: a real Error
/// (E0101) alongside an E0107 contradiction (Warning, never affects exit
/// code) still exits 1 both ways.
#[test]
fn exit_code_matches_with_and_without_format_flag() {
    let dir = tmpdir("exit_code_both_forms");
    write(&dir, "typo.rb", IVAR_TYPO);
    write(&dir, "contradiction.rb", CONTRADICTION);

    let normal = run_check(&dir, &[]);
    let agent = run_check(&dir, &["--format=agent"]);

    assert_eq!(normal.status.code(), Some(1), "normal stdout:\n{}", stdout(&normal));
    assert_eq!(agent.status.code(), Some(1), "agent stdout:\n{}", stdout(&agent));
}
