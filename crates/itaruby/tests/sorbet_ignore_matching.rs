//! H4b (2026-10-20): `sorbet/config` `--ignore` entries match exactly the
//! way sorbet 0.6.13437 was OBSERVED to match them (`srb tc --no-config
//! --dir . --ignore=<entry>` over a planted tree, one `T.reveal_type` per
//! file, reading which files it still typechecked). One test per observed
//! rule, each two-sided: the file Sorbet ignores loses its planted E0104,
//! the file Sorbet still checks keeps it, verbatim.
//!
//! Mutant IG-G in `scripts/ignore-indexing-mutants.sh` reverts the matcher
//! to the old whole-components-from-the-root comparison; these tests must
//! accuse it.

use std::path::{Path, PathBuf};
use std::process::Command;

fn tmpdir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Plants one unresolved constant per file, named after the file, then
/// runs `ita check` with `entry` as the only `--ignore`.
fn check_with_ignore(name: &str, entry: &str, files: &[(&str, &str)]) -> String {
    let dir = tmpdir(name);
    std::fs::create_dir_all(dir.join("sorbet")).unwrap();
    std::fs::write(dir.join("sorbet/config"), format!("--dir\n.\n--ignore={entry}\n")).unwrap();
    for (path, marker) in files {
        let full = dir.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, format!("{marker}\n")).unwrap();
    }
    let out = Command::new(env!("CARGO_BIN_EXE_ita"))
        .arg("check")
        .arg(&dir)
        .output()
        .expect("run `ita check`");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn warned(out: &str, marker: &str) -> bool {
    out.contains(&format!("warning[E0104]: unresolved constant `{marker}`"))
}

/// Observed: `--ignore=foo` ignores `foo/bar.rb` AND `bar/foo/baz.rb` — an
/// entry without a leading `/` matches anywhere in the relative path.
#[test]
fn unanchored_entry_matches_anywhere() {
    let out = check_with_ignore(
        "h4b_unanchored",
        "foo",
        &[("foo/bar.rb", "H4bTopFoo"), ("bar/foo/baz.rb", "H4bNestedFoo"), ("keep.rb", "H4bKeep")],
    );
    assert!(!warned(&out, "H4bTopFoo"), "`foo` must ignore foo/bar.rb:\n{out}");
    assert!(!warned(&out, "H4bNestedFoo"), "`foo` must ignore bar/foo/baz.rb:\n{out}");
    assert!(warned(&out, "H4bKeep"), "control: keep.rb is still checked:\n{out}");
}

/// Observed: `--ignore=/foo` ignores `foo/bar.rb` but still checks
/// `bar/foo/baz.rb` — a leading `/` anchors to the input dir.
#[test]
fn leading_slash_entry_is_anchored_prefix() {
    let out = check_with_ignore(
        "h4b_anchored",
        "/foo",
        &[("foo/bar.rb", "H4bAnchoredTop"), ("bar/foo/baz.rb", "H4bAnchoredNested")],
    );
    assert!(!warned(&out, "H4bAnchoredTop"), "`/foo` must ignore foo/bar.rb:\n{out}");
    assert!(warned(&out, "H4bAnchoredNested"), "`/foo` must still check bar/foo/baz.rb:\n{out}");
}

/// Observed: `--ignore=foo` still checks `foo.rb`, `bar/foo.rb` and
/// `foo2/bar.rb`, and `--ignore=oo` ignores nothing — a match covers
/// whole names only, on both ends.
#[test]
fn match_covers_whole_names_only() {
    let out = check_with_ignore(
        "h4b_whole_names",
        "foo",
        &[("foo.rb", "H4bFooFile"), ("bar/foo.rb", "H4bNestedFooFile"), ("foo2/bar.rb", "H4bFooTwo")],
    );
    assert!(warned(&out, "H4bFooFile"), "`foo` must not ignore foo.rb:\n{out}");
    assert!(warned(&out, "H4bNestedFooFile"), "`foo` must not ignore bar/foo.rb:\n{out}");
    assert!(warned(&out, "H4bFooTwo"), "`foo` must not ignore foo2/bar.rb:\n{out}");

    let out = check_with_ignore("h4b_partial_name", "oo", &[("foo/bar.rb", "H4bPartial")]);
    assert!(warned(&out, "H4bPartial"), "`oo` must ignore nothing:\n{out}");
}

/// Observed: `--ignore=a/b` ignores `a/b/c.rb` and `x/a/b/c.rb` but not
/// `a/b.rb`; `--ignore=foo.rb` ignores `bar/foo.rb` but not
/// `sub/foo.rb.bak/k.rb` — multi-segment and file-name entries follow the
/// same anywhere-but-whole-names rule.
#[test]
fn multi_segment_and_file_entries_match_anywhere() {
    let out = check_with_ignore(
        "h4b_multi_segment",
        "a/b",
        &[("a/b/c.rb", "H4bAbTop"), ("x/a/b/c.rb", "H4bAbNested"), ("a/b.rb", "H4bAbFile")],
    );
    assert!(!warned(&out, "H4bAbTop"), "`a/b` must ignore a/b/c.rb:\n{out}");
    assert!(!warned(&out, "H4bAbNested"), "`a/b` must ignore x/a/b/c.rb:\n{out}");
    assert!(warned(&out, "H4bAbFile"), "`a/b` must not ignore a/b.rb:\n{out}");

    let out = check_with_ignore(
        "h4b_file_entry",
        "foo.rb",
        &[("bar/foo.rb", "H4bFileEntry"), ("sub/foo.rb.bak/k.rb", "H4bFileEntryBak")],
    );
    assert!(!warned(&out, "H4bFileEntry"), "`foo.rb` must ignore bar/foo.rb:\n{out}");
    assert!(warned(&out, "H4bFileEntryBak"), "`foo.rb` must not ignore sub/foo.rb.bak/k.rb:\n{out}");
}

/// Observed: `--ignore=./foo` ignores nothing — entries are compared as
/// text, never normalized as paths.
#[test]
fn dot_slash_entry_matches_nothing() {
    let out = check_with_ignore("h4b_dot_slash", "./foo", &[("foo/bar.rb", "H4bDotSlash")]);
    assert!(warned(&out, "H4bDotSlash"), "`./foo` must ignore nothing:\n{out}");
}
