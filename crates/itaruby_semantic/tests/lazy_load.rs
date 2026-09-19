//! Bead B, onda 2: `run_load_hooks(<literal symbol>, <literal base>)`.
//!
//! `ActiveSupport.on_load(:sym) { ... }` stores a BLOCK under a symbol
//! and `run_load_hooks(:sym, Base)` hands `Base` to it; the library's
//! own `execute_hook` then runs it as `base.class_eval(&block)`
//! (`activesupport/lib/active_support/lazy_load_hooks.rb:107`). The
//! block and the base are decoupled by a symbol and the hook's receiver
//! is a method PARAMETER, so no per-`def` attribution can reach the
//! installed methods: `Base`'s real instance surface is whatever that
//! `class_eval` installs, and `NotFound` on it was measured wrong on
//! rails' `LazyLoadHooksTest::FakeContext` (three E0101 lines,
//! `activesupport/test/lazy_load_hooks_test.rb:137,169,174`).
//!
//! `apply_load_hook_openness` therefore marks the NAMED base open
//! (`Inconclusive`, never `NotFound`) and nothing else. Three controls
//! pin the keying, and each is a real `NoMethodError` under MRI:
//!
//! * a SIBLING class in the same file, same registry, same symbol, never
//!   handed to `run_load_hooks` — must keep accusing. This is the
//!   control that rejects "some file runs load hooks, stand every class
//!   down", the shape bead ita-a8z measured at 100% of two corpora.
//! * a THREE-argument call — not the library's shape at all, so the
//!   class it mentions second must stay closed.
//! * a hook NAME held in a variable — a registry this checker cannot
//!   follow.
//!
//! Fixtures live in `testdata/lazy_load/` with unique names (the gates
//! merge the whole `testdata/` tree into one project). Every fixture was
//! run under MRI: the silent one exits 0, each control raises
//! `NoMethodError` on the very line `ita check` blames.

use itaruby_semantic::{Db, ProjectFiles, SourceFile};

/// One fixture as its own single-file project, exactly like
/// `tests/included_hook.rs` — every error line carries the message only,
/// so an assertion names the method it is about.
fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/lazy_load");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let db = Db::default();
    let file = SourceFile::new(&db, path.into(), text);
    ProjectFiles::new(&db, vec![file]);
    itaruby_semantic::check_file(&db, file)
        .iter()
        .filter(|d| d.severity == itaruby_semantic::Severity::Error)
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect()
}

#[test]
fn run_load_hooks_base_resolves_silently() {
    let diags = check_fixture("run_load_hooks_base_resolves_silently.rb");
    assert!(
        diags.is_empty(),
        "the base handed to run_load_hooks has a class_eval'd surface, got: {diags:?}"
    );
}

#[test]
fn untouched_sibling_class_still_accuses() {
    let diags = check_fixture("untouched_sibling_class_still_accuses.rb");
    assert_eq!(diags.len(), 1, "exactly the untouched sibling must accuse: {diags:?}");
    assert!(
        diags[0].contains("E0101") && diags[0].contains("LazyHookSiblingHost::Untouched"),
        "expected E0101 naming the untouched sibling: {diags:?}"
    );
}

#[test]
fn three_argument_call_opens_nothing() {
    let diags = check_fixture("three_argument_call_opens_nothing.rb");
    assert_eq!(diags.len(), 1, "a three-argument call is not the library's shape: {diags:?}");
    assert!(
        diags[0].contains("E0101") && diags[0].contains("wrestler"),
        "expected E0101 on `wrestler`: {diags:?}"
    );
}

#[test]
fn non_literal_hook_name_opens_nothing() {
    let diags = check_fixture("non_literal_hook_name_opens_nothing.rb");
    assert_eq!(diags.len(), 1, "a hook name in a variable is unfollowable: {diags:?}");
    assert!(
        diags[0].contains("E0101") && diags[0].contains("wrestler"),
        "expected E0101 on `wrestler`: {diags:?}"
    );
}