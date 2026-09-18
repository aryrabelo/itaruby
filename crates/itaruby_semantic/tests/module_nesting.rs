//! Bead ita-519: `resolve_const`'s lexical scope used to be a flat string,
//! and walked "outer scopes" by truncating it at every `::` — treating each
//! segment of a class's own fully-qualified path as if it were a separately
//! opened `class`/`module` keyword. That is wrong for compact syntax:
//! `class A::B::C` gives `Module.nesting == [A::B::C]` (ONE level) in real
//! Ruby, never `[A, A::B, A::B::C]`. The bug: a bare reference inside such a
//! class silently resolved against a SIBLING class one level up (a real
//! class the project happens to define elsewhere) instead of a real
//! top-level class of the same name — 6 confirmed false positives in
//! rails/rails (`TestServer` inside a compact
//! `class ActionCable::Connection::AuthorizationTest` resolving to the
//! sibling `ActionCable::Connection::TestServer` instead of the real
//! top-level `::TestServer`).
//!
//! Fix: both `index.rs`'s `DefWalker` and `check.rs`'s `Checker` now thread
//! a REAL nesting stack (`Vec<String>`, one push per `class`/`module`
//! keyword actually written, compact or not) instead of deriving "outer
//! scopes" by splitting a flat path string. `ProjectIndex::resolve_const`
//! walks that real stack, innermost first, then toplevel — no truncation.
//!
//! Two-sided proof (a suppression rule proven on one side only is half a
//! measurement): `compact_sibling_resolves_to_toplevel.rb` proves the FP
//! dies (no diagnostic where the sibling would have wrongly closed the
//! call); `compact_real_error_still_accuses.rb` proves the fix hasn't gone
//! blind (a real E0101 on the correctly-resolved top-level target still
//! fires — a regression back to the sibling would swallow it silently);
//! `real_nesting_still_resolves_sibling.rb` proves genuinely nested
//! (non-compact) lexical scope still resolves through its real outer
//! levels, so the fix does not over-correct into single-level-only lookup.
//!
//! MUTANTS THIS FILE MUST CATCH:
//!
//!   1. `check.rs::scope_stmt`'s `ClassNode`/`ModuleNode` arms: the
//!      `child_scope.push(full.clone())` step replaced by pushing every
//!      `::`-segment of `full` as its own level (reintroducing the
//!      original bug) — `compact_sibling_resolves_to_toplevel` would
//!      wrongly diagnose E0101, and `compact_real_error_still_accuses`
//!      would wrongly go silent.
//!   2. Same site: `let mut child_scope = scope.to_vec();` replaced by
//!      `let mut child_scope = Vec::new();` (drop the outer real nesting
//!      entirely, not just avoid over-splitting `full`) —
//!      `real_nesting_still_resolves_sibling` would wrongly diagnose E0104.
//!   3. `index.rs::ProjectIndex::resolve_const`: the `for level in
//!      nesting.iter().rev()` walk reverted to string-truncation over
//!      `nesting.last()` — same effect as mutant 1, same two fixtures catch
//!      it.
//!   4. `index.rs::ProjectIndex::resolve_const`: the final toplevel
//!      fallback `self.by_path.get(name).copied()` removed —
//!      `compact_sibling_resolves_to_toplevel` would wrongly diagnose
//!      E0104 (the top-level class would never be found at all).

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/module_nesting_ita519");
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

#[test]
fn compact_class_syntax_resolves_bare_reference_to_toplevel_not_sibling() {
    let diags = check_fixture("compact_sibling_resolves_to_toplevel.rb");
    assert!(
        diags.is_empty(),
        "`Ita519Widget` referenced from inside the compact-syntax \
         `Ita519Outer::UsesCompact` must resolve to the TOP-LEVEL class \
         (Module.nesting has exactly one level for compact syntax), never \
         to the sibling `Ita519Outer::Ita519Widget` — got: {diags:?}"
    );
}

#[test]
fn compact_class_syntax_still_accuses_a_real_error_on_the_resolved_target() {
    let diags = check_fixture("compact_real_error_still_accuses.rb");
    assert_eq!(
        diags.len(),
        1,
        "expected exactly 1 diagnostic (a real E0101 on the correctly \
         resolved top-level class, which does not define `sibling_only`), \
         got: {diags:?}"
    );
    assert!(diags[0].contains("E0101"), "expected E0101, got: {:?}", diags[0]);
    assert!(
        diags[0].contains("sibling_only"),
        "message should name the undefined method, got: {:?}",
        diags[0]
    );
}

#[test]
fn genuinely_nested_lexical_scope_still_resolves_through_outer_levels() {
    let diags = check_fixture("real_nesting_still_resolves_sibling.rb");
    assert!(
        diags.is_empty(),
        "`Ita519Helper` referenced from inside the genuinely (non-compact) \
         nested `Ita519Real::UsesRealNesting` must still resolve through \
         the real outer `Ita519Real` nesting level: no diagnostic, got: {diags:?}"
    );
}
