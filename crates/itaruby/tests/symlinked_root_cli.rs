//! A root reached through a symlink must not fabricate diagnostics.
//!
//! Found 2026-08-24 while anchoring the corpus gate: one private corpus had
//! moved a directory level deeper, the gate's declared path was restored with
//! a symlink, and the same unchanged code went from 2 errors to **310**. Every
//! new one was a false positive of the same shape — ``` `undefined method `days`
//! for `Integer``, `present?` for `nil`, `camelize` for `String` ``` — i.e. the
//! `ActiveSupport` core extensions.
//!
//! The mechanism, and why it is the checker's bug and not the symlink's: the
//! closed-world gate (bead ita-2ve) makes "method not in the core inventory"
//! *conclusive* when no Gemfile is found within 4 levels above the root, and
//! that upward walk was purely lexical. Through a symlink the lexical parents
//! are the *link's* parents, so the real `Gemfile` sitting beside the real
//! `app/` is unreachable — and its absence was read as "this project has no
//! gems", which licenses calling every gem-added core method undefined.
//!
//! So absence of evidence was being read as evidence of absence, on the one
//! switch in the codebase that turns silence into errors. Invariant #1 says a
//! single false positive is a failure; this shape produces hundreds.
//!
//! Both sides are locked here: the symlinked root must stay silent (the fix),
//! and the same tree with the Gemfile genuinely absent must still accuse (the
//! control, without which the fix could just be "closed world never fires").

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn run_check(arg: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ita"))
        .arg("check")
        .arg(arg)
        .output()
        .expect("run `ita check`")
}

fn tmpdir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// `proj/Gemfile` + `proj/app/thing.rb`, where `thing.rb` calls a method that
/// only a gem could have added to a core class. Returns the tree root.
fn gem_project(name: &str, with_gemfile: bool) -> PathBuf {
    let dir = tmpdir(name);
    let app = dir.join("proj").join("app");
    std::fs::create_dir_all(&app).unwrap();
    if with_gemfile {
        std::fs::write(
            dir.join("proj").join("Gemfile"),
            "source \"https://rubygems.org\"\ngem \"activesupport\"\n",
        )
        .unwrap();
    }
    std::fs::write(
        app.join("thing.rb"),
        "class SymRootThing\n  def go\n    1.days\n  end\nend\n",
    )
    .unwrap();
    dir
}

fn diagnostic_count(out: &Output) -> usize {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.contains("error["))
        .count()
}

/// The silence side. Same files, same content, reached through a symlink:
/// the Gemfile is still there on disk, so the closed-world gate must still
/// see it and stay off.
#[cfg(unix)]
#[test]
fn symlinked_root_finds_the_real_gemfile_and_stays_silent() {
    let dir = gem_project("symroot_linked", true);
    let link = dir.join("linked-app");
    std::os::unix::fs::symlink(dir.join("proj").join("app"), &link).unwrap();

    let real = run_check(&dir.join("proj").join("app"));
    let linked = run_check(&link);

    assert_eq!(
        diagnostic_count(&real),
        0,
        "control: the real path must be silent, got:\n{}",
        String::from_utf8_lossy(&real.stdout)
    );
    assert_eq!(
        diagnostic_count(&linked),
        0,
        "a symlinked root must resolve to the same project, got:\n{}",
        String::from_utf8_lossy(&linked.stdout)
    );
    assert_eq!(
        real.status.code(),
        linked.status.code(),
        "same tree, same exit code"
    );
}

/// The accusing side, and the control that keeps the test above honest: with
/// no Gemfile anywhere above it, the gem set really is provably empty, and the
/// same call really is a `NoMethodError`. If this ever goes silent the fix has
/// stopped being a resolution fix and become a blanket suppression.
#[cfg(unix)]
#[test]
fn symlinked_root_without_a_gemfile_still_accuses() {
    let dir = gem_project("symroot_gemless", false);
    let link = dir.join("linked-app");
    std::os::unix::fs::symlink(dir.join("proj").join("app"), &link).unwrap();

    let out = run_check(&link);
    assert_eq!(
        diagnostic_count(&out),
        1,
        "gemless project must still report the undefined core method, got:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}
