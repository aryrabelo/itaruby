//! bead ita-9d4: `ita check` honors `sorbet/config`'s `--ignore` entries —
//! a subtree a project already declared out of scope for sorbet stays out
//! of scope for itaruby too, instead of drowning the real findings in
//! diagnostics from vendored/generated code sorbet itself never checks.
//!
//! Same tempdir pattern as `missing_path_cli.rs`: scratch trees under
//! cargo's `CARGO_TARGET_TMPDIR`, never `/tmp`.
//!
//! Mutants this file must kill:
//! (a) the ignore list is parsed but never applied to the walk — every
//!     "ignored file's diagnostic never appears" assertion below re-emits.
//! (b) component-boundary matching is replaced by substring/prefix
//!     matching — `ignore_entry_boundary_is_component_not_substring`'s
//!     `test/fixtures2/outside.rb` control wrongly vanishes, because
//!     `test/fix` would then match `test/fixtures2/...` as a raw prefix.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn run_check_json(dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ita"))
        .arg("check")
        .arg(dir)
        .arg("--format=json")
        .output()
        .expect("run `ita check --format=json`")
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

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

/// A bare top-level constant reference that resolves nowhere: the
/// cheapest possible planted diagnostic (E0104, a `Severity::Warning`
/// that never touches the exit code), one line, no project scaffolding
/// needed to make it fire.
fn marker(name: &str) -> String {
    format!("ItaSorbetIgnoreCli{name}\n")
}

/// I1, `--ignore=<path>` form. Two-sided in one test: the ignored file's
/// diagnostic never appears, and a sibling's diagnostic — planted the
/// same way, one directory over — still does. Without both sides, a
/// binary that ignored EVERYTHING would pass the first assertion alone.
#[test]
fn ignore_form_key_value_removes_matching_file() {
    let dir = tmpdir("ignore_kv_ita_9d4");
    write(&dir.join("sorbet/config"), "--ignore=vendor/\n");
    write(&dir.join("vendor/bad.rb"), &marker("Vendor"));
    write(&dir.join("good.rb"), &marker("Good"));

    let out = stdout(&run_check_json(&dir));
    assert!(
        !out.contains("ItaSorbetIgnoreCliVendor"),
        "ignored file's diagnostic must not appear, got:\n{out}"
    );
    assert!(
        out.contains("ItaSorbetIgnoreCliGood") && out.contains("\"code\":\"E0104\""),
        "sibling outside the ignore list must still be checked, got:\n{out}"
    );
}

/// I1, the two-token `--ignore <path>` form (sorbet's config file is
/// argv-style, one token per line) — same two-sided shape as the
/// `--ignore=<path>` test above, proving both forms are honored.
#[test]
fn ignore_form_space_separated_removes_matching_file() {
    let dir = tmpdir("ignore_space_ita_9d4");
    write(&dir.join("sorbet/config"), "--ignore\nvendor\n");
    write(&dir.join("vendor/bad.rb"), &marker("VendorSpace"));
    write(&dir.join("good.rb"), &marker("GoodSpace"));

    let out = stdout(&run_check_json(&dir));
    assert!(
        !out.contains("ItaSorbetIgnoreCliVendorSpace"),
        "ignored file's diagnostic must not appear (space form), got:\n{out}"
    );
    assert!(
        out.contains("ItaSorbetIgnoreCliGoodSpace"),
        "sibling outside the ignore list must still be checked (space form), got:\n{out}"
    );
}

/// I2: path-component boundary, both directions in one test.
/// - `test/fix` must NOT exclude `test/fixtures2/...` — a naive
///   string-prefix check would wrongly swallow it (mutant b).
/// - `test/fix` (no trailing slash) DOES exclude everything under
///   `test/fix/` — an entry names the directory whether or not it ends
///   in `/`.
#[test]
fn ignore_entry_boundary_is_component_not_substring() {
    let dir = tmpdir("ignore_boundary_ita_9d4");
    write(&dir.join("sorbet/config"), "--ignore=test/fix\n");
    write(&dir.join("test/fix/inside.rb"), &marker("Inside"));
    write(&dir.join("test/fixtures2/outside.rb"), &marker("Outside"));

    let out = stdout(&run_check_json(&dir));
    assert!(
        !out.contains("ItaSorbetIgnoreCliInside"),
        "`test/fix` (no trailing slash) must still exclude `test/fix/`, got:\n{out}"
    );
    assert!(
        out.contains("ItaSorbetIgnoreCliOutside"),
        "`test/fix` must NOT exclude `test/fixtures2/...` (component boundary), got:\n{out}"
    );
}

/// I3: no `sorbet/config` at all — the anti-overreach control. The same
/// tree that would be filtered by a `--ignore=vendor/` entry produces
/// every diagnostic when no config exists to opt into filtering, proving
/// `sorbet_ignore_entries` returning `vec![]` really is a no-op and not
/// merely "close".
#[test]
fn absent_sorbet_config_is_byte_identical() {
    let dir = tmpdir("ignore_absent_ita_9d4");
    write(&dir.join("vendor/bad.rb"), &marker("VendorAbsent"));
    write(&dir.join("good.rb"), &marker("GoodAbsent"));

    let out = stdout(&run_check_json(&dir));
    assert!(
        out.contains("ItaSorbetIgnoreCliVendorAbsent"),
        "no sorbet/config means nothing is filtered, got:\n{out}"
    );
    assert!(
        out.contains("ItaSorbetIgnoreCliGoodAbsent"),
        "no sorbet/config means nothing is filtered, got:\n{out}"
    );
}

/// An `--ignore` entry that matches no file on disk changes nothing:
/// every diagnostic that would have fired without the entry still fires
/// with it present. Rules out a broken matcher that accidentally excludes
/// unrelated files instead of doing nothing when its pattern is unused.
#[test]
fn ignore_entry_matching_nothing_changes_nothing() {
    let dir = tmpdir("ignore_noop_ita_9d4");
    write(&dir.join("sorbet/config"), "--ignore=nonexistent_subdir/\n");
    write(&dir.join("good.rb"), &marker("NoopControl"));

    let out = stdout(&run_check_json(&dir));
    assert!(
        out.contains("ItaSorbetIgnoreCliNoopControl"),
        "an ignore entry matching nothing must not suppress unrelated diagnostics, got:\n{out}"
    );
}
