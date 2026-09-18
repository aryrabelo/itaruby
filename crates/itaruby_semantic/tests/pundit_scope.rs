//! Bead ita-t6m: Pundit's `class Scope < Scope` idiom
//! (`app/policies/*_policy.rb`, chatwoot's `DataImportPolicy` among them —
//! every Rails app on Pundit writes this). Ruby evaluates a superclass
//! expression BEFORE the class being defined exists, so the RHS `Scope`
//! can never mean the class currently being opened — it resolves through
//! real lexical nesting first, then the ANCESTORS of the innermost
//! enclosing class (`DataImportPolicy < ApplicationPolicy`, so
//! `ApplicationPolicy::Scope` — reached via inheritance, not lexical
//! nesting, since the two `Scope` classes are not nested inside one
//! another at all).
//!
//! `ProjectIndex::resolve_const`'s plain lexical walk got this wrong: for
//! `class PunditScopeDataImportPolicy::Scope < Scope`, the walk's SECOND
//! nesting level reconstructs the candidate `PunditScopeDataImportPolicy::
//! Scope` — which is exactly the class being defined — and returns it
//! as if it were a legitimate resolution. `ancestors()` then linearizes
//! a self-referential one-class chain, reports `complete: true` (a
//! resolved-but-wrong superclass looks identical to "no superclass" to
//! the cycle guard), and every `scope`/`account`/`account_user`
//! `Scope` — which is exactly the class being defined — and returns it
//! `ApplicationPolicy::Scope`, never on the reopened `Scope` itself —
//! comes back `NotFound`: a conclusive, wrong E0101.
//!
//! Fix: `ProjectIndex::resolve_superclass_const` (and its `_lexical`/
//! `_fallback`/`_ancestor` helpers) replace `resolve_const` at all three
//! call sites that resolve a WRITTEN superclass name
//! (`build_subclass_map`, `linearize`, `collect_unresolved`). A lexical
//! candidate resolving to the class being defined is skipped, not
//! accepted; once lexical nesting is exhausted, the ancestors of the
//! innermost enclosing class are walked (nearest first) before a bare
//! top-level constant gets a look — so a real ancestor answer always
//! outranks an unrelated top-level same-named class.
//!
//! MUTANTS THIS FILE MUST CATCH:
//!
//!   (a) Revert to plain `resolve_const` at any of the three call sites
//!       (`build_subclass_map`/`linearize`/`collect_unresolved`) —
//!       `chatwoot_shape_silent` wrongly emits E0101 on `scope`/
//!       `account`/`account_user` (self-referential superclass, as
//!       described above).
//!   (b) `resolve_superclass_const` tries the bare top-level fallback
//!       BEFORE `resolve_superclass_ancestor` (i.e. "resolve to the
//!       outermost lexical `Scope`" instead of the real ancestor chain)
//!       — `outer_scope_vs_ancestor_scope` wrongly emits E0101: it
//!       lands on the empty top-level `Scope` decoy instead of
//!       `PunditScopeVsApplicationPolicy::Scope`, which actually
//!       defines `scope`.
//!   (c) `resolve_superclass_const` (or any of its helpers) made to
//!       always return `None` — i.e. the ancestry always stays open for
//!       ANY written superclass, "laziness" that would make every
//!       superclass Inconclusive — `real_error_still_accuses` wrongly
//!       goes silent: `totally_nonexistent_pundit_method` must still
//!       raise a conclusive E0101 once the ancestry genuinely IS fully
//!       resolved and closed.

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/pundit_scope");
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
fn chatwoot_shape_resolves_superclass_scope_through_ancestry() {
    let diags = check_fixture("chatwoot_shape_silent.rb");
    assert!(
        diags.is_empty(),
        "`class Scope < Scope` inside `PunditScopeDataImportPolicy` must \
         resolve the RHS through `PunditScopeApplicationPolicy`'s \
         ancestry, never to itself — `scope`/`account`/`account_user` \
         self-sends must stay silent, got: {diags:?}"
    );
}

#[test]
fn grandparent_scope_still_resolves_through_full_ancestor_chain() {
    let diags = check_fixture("grandparent_scope_silent.rb");
    assert!(
        diags.is_empty(),
        "the base `Scope` lives two levels up the ancestor chain \
         (grandparent policy); resolution must climb past the parent \
         (which defines no `Scope` of its own), got: {diags:?}"
    );
}

#[test]
fn real_error_inside_resolve_still_accuses() {
    let diags = check_fixture("real_error_still_accuses.rb");
    assert_eq!(
        diags.len(),
        1,
        "expected exactly 1 diagnostic (a real E0101 on a genuinely \
         undefined method, once the superclass DOES correctly resolve \
         and the ancestry IS fully closed), got: {diags:?}"
    );
    assert!(diags[0].contains("E0101"), "expected E0101, got: {:?}", diags[0]);
    assert!(
        diags[0].contains("totally_nonexistent_pundit_method"),
        "message should name the undefined method, got: {:?}",
        diags[0]
    );
}

#[test]
fn no_ancestor_defines_scope_stays_open_never_notfound() {
    let diags = check_fixture("no_ancestor_scope_silent.rb");
    assert!(
        diags.is_empty(),
        "when NO ancestor anywhere defines `Scope`, the superclass name \
         must stay unresolved and the ancestry must stay open — silence, \
         never a NotFound-driven E0101, got: {diags:?}"
    );
}

#[test]
fn ancestor_scope_outranks_unrelated_toplevel_scope() {
    let diags = check_fixture("outer_scope_vs_ancestor_scope.rb");
    assert!(
        diags.is_empty(),
        "a real ancestor answer (`PunditScopeVsApplicationPolicy::Scope`, \
         which defines `scope`) must outrank an unrelated top-level \
         `Scope` decoy that defines nothing — got: {diags:?}"
    );
}
