//! Bead ita-zdy: `x.is_a?(Foo)`'s false branch must eliminate `Foo` from
//! a CLOSED union (every member concrete, no `Ty::Unknown`) — Sorbet
//! does this elimination; its absence in itaruby caused a real false
//! positive against ruby-lsp's `declaration_listener.rb:512/515`
//! (`name_or_nesting.is_a?(Array) ? name_or_nesting :
//! Index.actual_nesting(@stack, name_or_nesting)`, a ternary whose false
//! branch feeds a `String`-only param). The general "`is_a`? false branch
//! has no information" contract (bead ita-u1t) stays exactly right for
//! `Ty::Unknown`/non-union receivers — this bead only adds the strictly
//! narrower closed-union case. Fixtures live under
//! `testdata/union_elimination/`, each with globally unique class names
//! (`UnionElim*` prefix) — `testdata/` is scanned as a single merged
//! project by `ita check testdata/` (gate c), so a name collision with
//! any other fixture in the tree would leak diagnostics across files.
//!
//! `check.rs`'s `union_elimination_tests` module has the direct unit
//! tests of the extracted `eliminate_union_member`/`ty_class_eq`
//! helpers, including the one control (a union literally containing
//! `Ty::Unknown`) that cannot be built from real Ruby source at all —
//! see that module's doc comment for why.

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/union_elimination");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap();
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, path.clone().into(), text);
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    // Closed-world ON (bead ita-2ve): the ternary fixture's `is_a?(Array)`
    // is a core-class check, only narrowed under closed-world — same
    // setup `case_narrowing.rs` uses for its core-class fixtures. The
    // project-class fixtures don't need it, but turning it on
    // unconditionally keeps this file's setup identical to every other
    // narrowing test file rather than a special case per fixture.
    itaruby_semantic::ClosedWorld::new(&db, true);
    itaruby_semantic::check_file(&db, file)
        .iter()
        .map(|d| format!("{}[{}]: {}", if d.severity == itaruby_semantic::Severity::Error { "error" } else { "warning" }, d.code, d.message))
        .collect()
}

/// FIXTURE 1 (the ruby-lsp shape): a ternary `x.is_a?(Array) ? x :
/// take_string(x)` where `x : String | Array[String]`. The false branch
/// feeds a `String`-only param — SILENT only if `Array` was eliminated
/// from the union first. MUTANT this test catches: disabling the
/// elimination entirely (`apply_narrow_false`'s `IsA` arm becomes a
/// no-op again) resurrects the false positive: `x` stays `String |
/// Array[String]`, and `compatible` rejects the `Array[String]` half
/// against the `String` param as E0103.
#[test]
fn ternary_is_a_array_false_branch_eliminates_array_from_the_union() {
    let diags = check_fixture("ternary_ruby_lsp_shape_silent.rb");
    assert!(
        diags.is_empty(),
        "ternary false branch must narrow String | Array[String] down to String, got: {diags:?}"
    );
}

/// FIXTURE 2: plain `if`/`else` (not a ternary) on a closed
/// project-class union `UnionElimIfA | UnionElimIfB`. The false branch
/// eliminates `UnionElimIfA`, leaving exactly `UnionElimIfB` — passed to
/// a sink method valid ONLY on that remaining member. SILENT only if the
/// elimination happened; without it `compatible` would reject the `A`
/// half of the still-full union against the `B`-only param.
#[test]
fn if_false_branch_narrows_closed_union_and_stays_silent() {
    let diags = check_fixture("if_false_branch_narrows_and_stays_silent.rb");
    assert!(
        diags.is_empty(),
        "if/else false branch must narrow UnionElimIfA | UnionElimIfB down to UnionElimIfB, got: {diags:?}"
    );
}

/// FIXTURE 3 (control): a declared union with one unresolvable member
/// (`UnionElimUnknownExternalGemClass`, never defined anywhere in
/// `testdata/`) collapses to plain `Ty::Unknown` before it ever reaches
/// the elimination — `Ty::union` absorbs `Unknown` outright, per
/// `types.rs`. A call invalid on the only concrete member
/// (`only_on_sink_class` doesn't exist; the false branch calls a
/// different, genuinely nonexistent method) must stay silent —
/// invariant #1: `Ty::Unknown` never produces a diagnostic. This proves
/// the end-to-end consequence; `check.rs`'s `union_elimination_tests`
/// module proves the underlying guard directly, since a `Ty::Union`
/// containing `Ty::Unknown` cannot be reached through real source at
/// all (see that module's doc comment).
#[test]
fn union_with_unresolvable_member_collapses_to_unknown_and_stays_silent() {
    let diags = check_fixture("unresolved_member_control_silent.rb");
    assert!(
        diags.is_empty(),
        "a union with an unresolvable member must degrade to Unknown and never manufacture a diagnostic, got: {diags:?}"
    );
}

/// FIXTURE 4 (control): elimination must remove the TESTED member, not
/// the other one. `only_on_a` exists on `UnionElimAccuseA`, not on
/// `UnionElimAccuseB`. The false branch of `x.is_a?(UnionElimAccuseA)`
/// must narrow `x` to exactly `UnionElimAccuseB` — calling `only_on_a`
/// there is genuinely invalid and MUST still fire E0101. MUTANT this
/// test catches: an implementation that keeps the tested member and
/// drops the other one instead (e.g. `retain(|p| ty_class_eq(p,
/// tested))` instead of `retain(|p| !ty_class_eq(p, tested))`) would
/// narrow `x` to `UnionElimAccuseA`, where `only_on_a` is valid — 0
/// diagnostics instead of the expected 1.
#[test]
fn false_branch_narrows_to_correct_member_and_still_accuses_e0101() {
    let diags = check_fixture("false_branch_elimination_still_accuses.rb");
    assert_eq!(
        diags.len(),
        1,
        "the true branch's call is valid (UnionElimAccuseA#only_on_a exists); only the false branch's call on the narrowed UnionElimAccuseB must fire, got: {diags:?}"
    );
    assert!(diags[0].contains("E0101"), "expected E0101, got: {diags:?}");
    assert!(diags[0].contains("only_on_a"), "expected the diagnostic to name only_on_a, got: {diags:?}");
    assert!(
        diags[0].contains("UnionElimAccuseB"),
        "expected the diagnostic to blame UnionElimAccuseB (the correctly-narrowed remaining member), got: {diags:?}"
    );
}
