//! End-to-end CLI tests for the `ita check` declaration-source announcement
//! (bead ita-2a9): real binary, real discovery, real tempdir fixtures. The
//! LSP side already announces what `wire_declaration_sources` found, one
//! `window/logMessage` per source, via `itaruby_server::main_loop::
//! send_discovery_log`; this bead gives the CLI the same visibility on
//! stderr, unconditionally (no flag), with byte-identical wording to the
//! LSP's per-source lines — see `main.rs::announce_discovered_sources`.
//! Same tempdir pattern as `core_closed_world_cli.rs`/`stats_cli.rs`:
//! scratch trees live under cargo's own `CARGO_TARGET_TMPDIR`, never
//! `/tmp`, so no fixture here ever sits upward of another's discovery walk.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn run_check(dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ita"))
        .arg("check")
        .arg(dir)
        .output()
        .expect("run `ita check`")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
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

/// A project with a `db/schema.rb` announces it on stderr, by default, with
/// the exact path discovery found — the LSP-mirrored line
/// (`itaruby: discovered <path> (schema.rb)`) appears nowhere unless
/// discovery actually found something, so this substring can only go green
/// because the announcement fired.
#[test]
fn schema_rb_announces_on_stderr_with_its_path() {
    let dir = tmpdir("announce_schema");
    std::fs::create_dir_all(dir.join("db")).unwrap();
    write(
        &dir,
        "db/schema.rb",
        "ActiveRecord::Schema.define(version: 1) do\n  create_table \"widgets\" do |t|\n    t.string \"name\"\n  end\nend\n",
    );

    let out = run_check(&dir);
    let schema_path = dir.join("db").join("schema.rb");
    let expected = format!("itaruby: discovered {} (schema.rb)", schema_path.display());
    assert!(
        stderr(&out).contains(&expected),
        "expected stderr to announce the schema.rb path, got:\n{}",
        stderr(&out)
    );
}

/// A project with no `db/schema.rb`, `db/structure.sql`, or `sorbet/rbi`
/// anywhere upward prints nothing about sources: total silence, not an
/// explicit "found nothing" line (that's the LSP's `send_discovery_log`
/// behavior, deliberately not mirrored here — see `announce_discovered_
/// sources`'s doc comment).
#[test]
fn no_declaration_source_prints_nothing_about_sources() {
    let dir = tmpdir("announce_none");
    write(&dir, "plain.rb", "class Widget\nend\n");

    let out = run_check(&dir);
    assert!(
        !stderr(&out).contains("itaruby: discovered"),
        "expected no source announcement, got stderr:\n{}",
        stderr(&out)
    );
    assert!(
        stderr(&out).is_empty(),
        "expected fully silent stderr with no discoverable source, got:\n{}",
        stderr(&out)
    );
}

/// The announcement lives on stderr only — `--format=json`'s bare-stdout
/// and the plain channel's diagnostic-only stdout are both byte-frozen
/// contracts (w12/contract-2), so the `itaruby: discovered` line the
/// schema.rb fixture proves onto stderr above must never leak onto stdout.
#[test]
fn announcement_never_leaks_onto_stdout() {
    let dir = tmpdir("announce_stdout_clean");
    std::fs::create_dir_all(dir.join("db")).unwrap();
    write(
        &dir,
        "db/schema.rb",
        "ActiveRecord::Schema.define(version: 1) do\n  create_table \"widgets\" do |t|\n    t.string \"name\"\n  end\nend\n",
    );

    let out = run_check(&dir);
    assert!(
        !stdout(&out).contains("itaruby: discovered"),
        "the announcement leaked onto stdout: {}",
        stdout(&out)
    );
}
