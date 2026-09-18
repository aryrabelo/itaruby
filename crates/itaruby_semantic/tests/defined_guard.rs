//! Bead ita-r8k: `defined?(X) ? X : Y` (and its `unless` mirror) flags the
//! constant read `X` in the branch that only ever runs once `defined?`
//! already proved `X` exists at runtime — measured as a false positive on
//! rails (2 sites, e.g. `defined?(::AppBuilder) ? ::AppBuilder :
//! Rails::AppBuilder`). Fix: `Checker::defined_guards` (a stack of literal
//! constant paths proven by an enclosing `defined?` guard) is pushed
//! before walking exactly the branch the guard covers — `IfNode`'s
//! then-branch (also covers ternary, which prism parses as the same
//! `IfNode` shape) and `UnlessNode`'s else-clause — and popped right
//! after. `check_const_ref` treats membership as a suppression-only
//! channel, same contract as every RBI channel beside it: it never
//! changes what `infer_const` types the reference as (invariant #1).
//!
//! The mutants this fixture set must catch:
//!   1. the suppression removed entirely (`defined_guards` never pushed,
//!      or `check_const_ref` never consults it) -> both
//!      `if_ternary_guard_is_silent` and `unless_else_guard_is_silent`
//!      re-emit the historical E0104.
//!   2. the suppression widened to the WRONG branch (`unless`'s own body
//!      instead of its else-clause, or `if`'s else instead of its then)
//!      -> `if_else_branch_still_accuses` goes silent.
//!   3. the suppression matches ANY defined?-guarded branch regardless of
//!      which constant the guard named (e.g. keyed on "a guard is active"
//!      instead of the exact path) -> `guard_wrong_constant_still_accuses`
//!      goes silent.
//!   4. `check_const_ref`'s guard check short-circuits ALL unresolved
//!      constants, guarded or not -> `unguarded_missing_still_accuses`
//!      goes silent (also the sanity check every other test here depends
//!      on: if this ever goes silent, the fix degraded into blanket
//!      suppression).

fn dir() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/defined_guard")
}

/// Mirrors `const_visibility.rs`'s own single-file harness: one file, one
/// throwaway `Db`, diagnostics rendered as `line:col CODE message`.
fn check_single(name: &str) -> Vec<String> {
    let dir = dir();
    let db = itaruby_semantic::Db::default();
    let text = std::fs::read_to_string(format!("{dir}/{name}")).unwrap();
    let file = itaruby_semantic::SourceFile::new(&db, format!("{dir}/{name}").into(), text.clone());
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

/// Fixture (a), mutant 1: `defined?(::X) ? ::X : Y` — the rails shape.
/// The then-branch read of `::DefinedGuardExtTernary` must be silent; the
/// else-branch fallback resolves normally through the project index and
/// is silent for an unrelated, pre-existing reason.
#[test]
fn if_ternary_guard_is_silent() {
    let diags = check_single("if_ternary_guard_silent.rb");
    assert!(diags.is_empty(), "defined?-guarded then-branch read must not warn, got: {diags:?}");
}

/// Fixture (b), mutant 1: the `unless` mirror — `unless defined?(::X);
/// Y; else; ::X; end`. The else-clause read of
/// `::DefinedGuardExtUnless` runs only when `defined?` proved it exists.
#[test]
fn unless_else_guard_is_silent() {
    let diags = check_single("unless_else_guard_silent.rb");
    assert!(diags.is_empty(), "defined?-guarded unless else-clause read must not warn, got: {diags:?}");
}

/// Fixture (c), mutant 2: the ELSE-branch of `if defined?(X)` runs when
/// `defined?` proved X does NOT exist — the read of X there is genuinely
/// unsafe and must still warn.
#[test]
fn if_else_branch_still_accuses() {
    let diags = check_single("if_else_branch_still_accuses.rb");
    assert_eq!(diags.len(), 1, "the else-branch read is genuinely unsafe: {diags:?}");
    assert!(diags[0].contains("E0104"), "expected E0104, got: {diags:?}");
    assert!(diags[0].contains("DefinedGuardElseUnsafe"), "expected the unsafe read named, got: {diags:?}");
}

/// Fixture (d), mutant 3: `defined?(A) ? B : nil` — the guard names A,
/// the then-branch reads a DIFFERENT constant B. B must still warn: the
/// suppression is keyed on the exact guarded path, never "some defined?
/// guard is active in this branch".
#[test]
fn guard_wrong_constant_still_accuses() {
    let diags = check_single("guard_wrong_constant_still_accuses.rb");
    assert_eq!(diags.len(), 1, "guard on A must not silence a read of the different constant B: {diags:?}");
    assert!(diags[0].contains("E0104"), "expected E0104, got: {diags:?}");
    assert!(diags[0].contains("DefinedGuardB"), "expected the unguarded constant B named, got: {diags:?}");
}

/// Fixture (e), mutant 4 (sanity check every test above depends on): a
/// plain unguarded reference to a constant that exists nowhere in the
/// project must still warn.
#[test]
fn unguarded_missing_still_accuses() {
    let diags = check_single("unguarded_missing_still_accuses.rb");
    assert_eq!(diags.len(), 1, "an unguarded unresolved constant must still warn: {diags:?}");
    assert!(diags[0].contains("E0104"), "expected E0104, got: {diags:?}");
    assert!(diags[0].contains("DefinedGuardMissing"), "expected the missing constant named, got: {diags:?}");
}
