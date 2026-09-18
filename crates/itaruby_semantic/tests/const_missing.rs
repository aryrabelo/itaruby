//! A namespace defining `self.const_missing` autovivifies constants at
//! runtime, so `Plugins::Markdown` resolves and the program runs clean.
//! `E0104` there read absence of evidence as evidence of absence — a live
//! invariant #1 violation, found by the inference bench
//! (`scripts/inference-bench.jsonl`, case `const_missing_namespace`) and
//! fixed in `check_const_ref` as a suppression-only channel.
//!
//! Two-sided, and scoped on the axis where being wrong would COST a true
//! positive: the hook silences only a QUALIFIED reference, only through its
//! IMMEDIATE parent namespace, and only when project code really defines
//! the hook. Fixtures in `testdata/const_missing/`.

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/const_missing");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap();
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, path.into(), text.clone());
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    let li = itaruby_semantic::LineIndex::new(&text);
    itaruby_semantic::check_file(&db, file)
        .iter()
        .map(|d| {
            let (l, c) = li.line_col(&text, d.start);
            format!("{}:{}:{} {}", l + 1, c + 1, d.code, d.message)
        })
        .collect()
}

/// The fix: the owning namespace defines the hook, so the reference is
/// unprovable, not absent. Silent.
#[test]
fn const_missing_hook_silences_qualified_reference() {
    let diags = check_fixture("hook_silences_qualified_ref.rb");
    assert!(diags.is_empty(), "invariant #1: expected silence, got: {diags:?}");
}

/// Ruby calls `const_missing` on the CREF, so a bare reference inside the
/// module that defines the hook resolves at runtime (the fixture exits 0
/// under MRI). Warning here was a second, separate false positive.
#[test]
fn bare_reference_inside_the_hook_namespace_is_silent() {
    let diags = check_fixture("bare_ref_inside_hook_namespace_silent.rb");
    assert!(diags.is_empty(), "invariant #1: expected silence, got: {diags:?}");
}

/// The cref is consulted ALONE — the chain is not walked outward. Hook on
/// `Outer`, bare reference inside `Outer::Inner`: MRI raises
/// `uninitialized constant Outer::Inner::Widget` at line 12, so must we.
#[test]
fn bare_reference_outside_the_hook_namespace_still_warns() {
    let diags = check_fixture("bare_ref_outside_hook_namespace_still_warns.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert_eq!(
        diags[0], "12:7:E0104 unresolved constant `Widget`",
        "an outer namespace's hook must not silence a nested bare reference"
    );
}

/// The qualified counterpart: nesting is not inheritance. For
/// `Outer::Inner::Widget` MRI calls `Inner.const_missing`, which does not
/// exist, so the grandparent's hook never runs.
#[test]
fn grandparent_hook_does_not_silence_a_qualified_reference() {
    let diags = check_fixture("grandparent_qualified_still_warns.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert_eq!(
        diags[0], "14:1:E0104 unresolved constant `Outer::Inner::Widget`",
        "only the immediate parent's hook governs a qualified reference"
    );
}

/// The control that keeps the fix from being a blanket silencer: same
/// shape, no hook, so the constant really is unresolved.
#[test]
fn namespace_without_hook_still_warns() {
    let diags = check_fixture("no_hook_still_warns.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0104"), "expected E0104, got: {}", diags[0]);
    assert!(
        diags[0].contains("Plugins::Markdown"),
        "message should name the constant, got: {}",
        diags[0]
    );
}

/// Scope control: a hook on a DIFFERENT namespace in the same file must not
/// silence this one. A single `const_missing` anywhere never opens every
/// namespace around it.
#[test]
fn sibling_namespace_hook_does_not_silence() {
    let diags = check_fixture("sibling_namespace_hook_still_warns.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0104"), "expected E0104, got: {}", diags[0]);
}
