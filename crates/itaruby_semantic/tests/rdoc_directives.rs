//! Bead ita-ekg: `#:nodoc:` / `#:doc:` (`RDoc` visibility directives) must
//! never be parsed as RBS sigs. Measured real site: zammad's
//! `lib/core_ext/mail/fields/common_date_field.rb:15` —
//! `class CommonDateField < NamedStructuredField #:nodoc:` — fired E0105
//! ("expected `->` at offset 0"), because `index.rs`'s `sig_comments`
//! collector keys any `#:`-prefixed comment by the line it sits on, then
//! attaches whichever comment sits directly above a `def` as that def's
//! sig; a trailing `#:nodoc:` on the class line one line above a `def`
//! was handed straight to `parse_rbs_comment`, which of course rejects
//! `nodoc:` as a signature.
//!
//! The fix lives at the COLLECTION layer (`index.rs`, not
//! `rbs_comment.rs`'s parser): a `#:` comment is only a sig candidate if
//! the character immediately after `#:` (untrimmed — no separating
//! whitespace) is NOT a word character. No valid RBS sig form starts with
//! a bare letter/digit/underscore right after `#:` — every real form
//! begins with `(`, `[`, or `-` (of `->`); see `rbs_comment.rs`'s own
//! `malformed` test, where even `"Integer -> String"` (no parens) is
//! rejected by the parser itself. A comment with a real space after `#:`
//! (`#: String ->`) is a genuine sig attempt and must still be attempted
//! (and still fail if malformed).
//!
//! MUTANTS THIS FILE MUST CATCH:
//!   1. The glued-word filter removed/disabled entirely — the collector
//!      goes back to inserting every `#:`-prefixed comment unconditionally
//!      — `class_nodoc_directive_is_silent` and
//!      `doc_directive_on_def_line_is_silent` flip from 0 E0105s to 1
//!      each (the historical zammad false positive re-emitted).
//!   2. The filter made over-broad — skip ANY `#:` followed by a letter
//!      ANYWHERE in the trimmed body, instead of only the untrimmed
//!      glued case — `spaced_malformed_sig_still_warns` flips from 1
//!      E0105 to 0 (a genuine `#: String ->` sig attempt would go
//!      silent).
//!   3. The filter widened to also skip glued non-word starts (`(`, `[`,
//!      `-`) — `glued_paren_sig_still_attaches_and_checks` loses its
//!      E0102/E0103, because the sig comment never attaches to
//!      `RdocDirValidSigArity#accept` / `RdocDirValidSigType#accept` at
//!      all.

fn diags_of(name: &str, text: &str) -> Vec<String> {
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, format!("/p/{name}").into(), text.to_string());
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    itaruby_semantic::check_file(&db, file)
        .iter()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect()
}

fn check_fixture(name: &str) -> Vec<String> {
    let base = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/rdoc_directives");
    let path = format!("{base}/{name}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    diags_of(name, &text)
}

/// Fixture 1 — the zammad shape itself: `#:nodoc:` glued to a class
/// declaration line, immediately followed by a `def`. Zero E0105.
#[test]
fn class_nodoc_directive_is_silent() {
    let diags = check_fixture("class_nodoc_silent.rb");
    let e0105: Vec<_> = diags.iter().filter(|d| d.starts_with("E0105")).collect();
    assert!(e0105.is_empty(), "expected zero E0105, got: {e0105:?} (all diags: {diags:?})");
}

/// Fixture 2 — `#:doc:` glued directly to a `def` line (not a class
/// line), with another `def` on the very next line. Zero E0105.
#[test]
fn doc_directive_on_def_line_is_silent() {
    let diags = check_fixture("doc_directive_on_def_line_silent.rb");
    let e0105: Vec<_> = diags.iter().filter(|d| d.starts_with("E0105")).collect();
    assert!(e0105.is_empty(), "expected zero E0105, got: {e0105:?} (all diags: {diags:?})");
}

/// Fixture 3 (control) — a real sig attempt with a space after `#:`
/// (`#: String ->`) must keep accusing E0105 when malformed: the fix must
/// never touch spaced comments, only glued ones.
#[test]
fn spaced_malformed_sig_still_warns() {
    let diags = check_fixture("malformed_sig_still_warns.rb");
    let e0105: Vec<_> = diags.iter().filter(|d| d.starts_with("E0105")).collect();
    assert_eq!(e0105.len(), 1, "expected exactly 1 E0105, got: {diags:?}");
}

/// Fixture 4 (control) — a glued-paren sig (`#:(Type) -> void`, no space,
/// but `(` can never start an `RDoc` directive) must still attach and
/// drive real checks: wrong arg count fires E0102, wrong arg type fires
/// E0103. Proves the fix does not over-skip every glued `#:` comment,
/// only glued WORD ones.
#[test]
fn glued_paren_sig_still_attaches_and_checks() {
    let diags = check_fixture("valid_sig_still_works.rb");
    assert!(!diags.iter().any(|d| d.starts_with("E0105")), "unexpected E0105: {diags:?}");
    assert!(
        diags.iter().any(|d| d.starts_with("E0102")),
        "expected an E0102 (wrong arity) proving the glued sig's def still resolves and checks normally, got: {diags:?}"
    );
    assert!(
        diags.iter().any(|d| d.starts_with("E0103")),
        "expected an E0103 (arg type mismatch) proving the glued-paren sig actually attached, got: {diags:?}"
    );
}
