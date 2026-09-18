//! `ita check` on a root that does not exist must fail loudly (bead
//! ita-76m.2). Before this, `discover_rb_files`'s `read_dir` walk turned a
//! missing path into zero files, which turned into zero diagnostics and
//! exit 0 — indistinguishable from "your code is clean", and a green CI
//! job that checked nothing. The distinction this file locks is the whole
//! point: a *missing* root is a usage error; an *existing* root with no
//! `.rb` in it is legitimately clean and must stay silent at exit 0.
//!
//! Same tempdir pattern as `discovery_announce.rs`/`stats_cli.rs`: scratch
//! trees under cargo's `CARGO_TARGET_TMPDIR`, never `/tmp`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn run_check(arg: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ita"))
        .arg("check")
        .arg(arg)
        .output()
        .expect("run `ita check`")
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

/// The accusing side. A path that was never on disk exits 2 (the same
/// usage class as an unknown `--format=`) and names the path, so the
/// operator can see *which* argument was wrong rather than reading an
/// empty successful run.
#[test]
fn missing_root_exits_nonzero_and_names_the_path() {
    let missing = Path::new(env!("CARGO_TARGET_TMPDIR")).join("no_such_root_ita_76m2");
    let _ = std::fs::remove_dir_all(&missing);

    let out = run_check(missing.to_str().unwrap());
    assert_eq!(
        out.status.code(),
        Some(2),
        "a missing root must not exit 0, got {:?} with stderr:\n{}",
        out.status.code(),
        stderr(&out)
    );
    assert!(
        stderr(&out).contains(&format!("path not found: {}", missing.display())),
        "stderr must name the missing path, got:\n{}",
        stderr(&out)
    );
    assert!(
        out.stdout.is_empty(),
        "a rejected run must produce no diagnostics on stdout, got:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// The silence side, and the reason the check is `exists()` rather than
/// "found no Ruby". A directory that really is there and really has no
/// `.rb` is a clean run: exit 0, nothing on either stream. Widening the
/// rejection to cover this case would break checking a subtree that has
/// no Ruby yet.
#[test]
fn existing_but_empty_root_stays_silent_at_exit_zero() {
    let dir = tmpdir("empty_root_ita_76m2");

    let out = run_check(dir.to_str().unwrap());
    assert_eq!(
        out.status.code(),
        Some(0),
        "an existing empty directory is a clean run, got {:?} with stderr:\n{}",
        out.status.code(),
        stderr(&out)
    );
    assert!(
        stderr(&out).is_empty(),
        "an existing empty directory must print nothing, got stderr:\n{}",
        stderr(&out)
    );
    assert!(out.stdout.is_empty(), "expected no diagnostics on stdout");
}

/// A directory that exists and holds Ruby with nothing wrong in it is also
/// exit 0 — this is the control that keeps the two tests above from both
/// passing on a binary that simply rejects everything.
#[test]
fn existing_root_with_clean_ruby_is_still_exit_zero() {
    let dir = tmpdir("clean_root_ita_76m2");
    std::fs::write(dir.join("ok.rb"), "class Ita76m2Widget\nend\n").unwrap();

    let out = run_check(dir.to_str().unwrap());
    assert_eq!(
        out.status.code(),
        Some(0),
        "clean Ruby must exit 0, got {:?} with stderr:\n{}",
        out.status.code(),
        stderr(&out)
    );
}

/// The reason the guard is `try_exists` and not `exists()`: a root we
/// cannot even ask about is not the same failure as a root that is not
/// there. `exists()` returns `false` for both, so an unreadable path would
/// be reported as a typo and send the operator hunting the wrong bug.
/// Both exit 2 — both mean this run checked nothing — but the message has
/// to name which one happened.
///
/// Unix-only: the fixture is a parent directory with its traversal bit
/// removed, which is how you make `stat` fail with `EACCES` rather than
/// `ENOENT`.
///
/// The skip condition is the precondition itself, measured after the
/// `chmod`: if this process can still stat through a `0o000` directory,
/// the fixture did not take, and the test has nothing to say. That is
/// true for root, and equally true on a filesystem that ignores
/// permission bits — a `geteuid`-based skip only catches the first and
/// would fail confusingly on the second, and it needed an `extern "C"`
/// block to ask.
#[cfg(unix)]
#[test]
fn unreadable_path_says_cannot_read_not_not_found() {
    use std::os::unix::fs::PermissionsExt;

    let parent = tmpdir("unreadable_parent_ita_76m2");
    let child = parent.join("child");
    std::fs::create_dir_all(&child).unwrap();
    std::fs::write(child.join("ok.rb"), "class Ita76m2Locked\nend\n").unwrap();
    // Drop the parent's traversal bit: stat on `parent/child` now fails
    // with EACCES instead of answering yes or no.
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o000)).unwrap();

    let fixture_took = std::fs::metadata(&child).is_err();
    if !fixture_took {
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!(
            "skipped: this process still traverses a 0o000 directory (root, or a \
             filesystem that ignores permission bits), so the fixture cannot exist here"
        );
        return;
    }

    let out = run_check(child.to_str().unwrap());
    let err = stderr(&out);
    // Restore before asserting, so a failure never leaves an
    // unremovable directory behind for the next run.
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(
        out.status.code(),
        Some(2),
        "an unreadable root must exit 2, got {:?} with stderr:\n{err}",
        out.status.code()
    );
    assert!(
        err.contains("cannot read path:"),
        "an unreadable root must be reported as unreadable, got:\n{err}"
    );
    assert!(
        !err.contains("path not found:"),
        "an unreadable root must NOT be reported as a missing path — that is \
         exactly the conflation `try_exists` exists to break, got:\n{err}"
    );
}
