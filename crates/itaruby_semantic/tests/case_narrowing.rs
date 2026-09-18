//! `case`/`when` class-dispatch narrowing and the truthy/or-guard forms
//! `narrow_of` couldn't see before (bead ita-w9i): `case x; when Foo; ...`
//! narrows `x` to `Foo` inside that branch; a bare truthy predicate (`if
//! x`, `unless x`, ternary `x ? a : b`) narrows the same way `x.nil?`
//! already did; `x.nil? || <anything>` survives an early-return guard.
//! Fixtures live under `testdata/case_narrowing/`, each with globally
//! unique class names — `testdata/` is scanned as a single merged project
//! by `ita check testdata/` (gate c), so a name collision with any other
//! fixture in the tree would leak diagnostics across files.

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/case_narrowing");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap();
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, path.clone().into(), text);
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    // Closed-world ON (bead ita-2ve): `truthy_false_branch_silent.rb`
    // exists specifically to prove the `Truthy` kind's false branch is a
    // no-op even when a wrong implementation would manufacture `Nil` —
    // and calling an unknown method on a concrete `Ty::Nil` receiver is
    // only ever conclusive (E0101) under closed-world, per
    // `Checker::core_unknown_is_conclusive`. Off, that specific mutant
    // would go undetected by construction, not because the fix is right.
    itaruby_semantic::ClosedWorld::new(&db, true);
    itaruby_semantic::check_file(&db, file)
        .iter()
        .map(|d| format!("{}[{}]: {}", if d.severity == itaruby_semantic::Severity::Error { "error" } else { "warning" }, d.code, d.message))
        .collect()
}

/// FIRES: `case event; when SubA; ...` narrows `event` to `SubA` inside
/// that branch — an unknown method there is a real E0101.
#[test]
fn case_class_dispatch_narrows_and_catches_unknown_method() {
    let diags = check_fixture("case_class_dispatch_catches_unknown_method.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0101"), "expected E0101, got: {diags:?}");
    assert!(diags[0].contains("nonexistent_method"), "got: {diags:?}");
}

/// FIRES: `when A, B` narrows the subject to the union `A | B`, not to
/// `A` alone and not back to Unknown — passing it to a method that only
/// accepts `A` must catch the `B` half as a real E0103.
#[test]
fn case_multi_class_when_narrows_to_union_and_catches_arg_mismatch() {
    let diags = check_fixture("case_multi_class_when_narrows_union.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0103"), "expected E0103, got: {diags:?}");
    assert!(diags[0].contains("CaseNarUnionA") && diags[0].contains("CaseNarUnionB"), "got: {diags:?}");
}

/// SILENT: falling through every `when` proves nothing about the
/// subject — the `else` branch must stay unrefined, not be narrowed to
/// some other guessed type.
#[test]
fn case_else_branch_has_no_narrowing_info() {
    let diags = check_fixture("case_else_silent.rb");
    assert!(diags.is_empty(), "case else branch must stay silent, got: {diags:?}");
}

/// SILENT: a `when` clause with one resolvable class condition and one
/// unresolvable condition (a `Range`) must narrow NEITHER — never narrow
/// off of only the conditions that did resolve.
#[test]
fn case_when_with_unresolvable_condition_narrows_nothing() {
    let diags = check_fixture("case_unresolvable_when_silent.rb");
    assert!(diags.is_empty(), "partially-unresolvable when clause must stay silent, got: {diags:?}");
}

/// FIRES (bead ita-w9i contract: "`return unless x` narrows the rest of
/// the method the same way `return if x.nil?` already did"): the bare
/// truthy `unless`-guard idiom must narrow `x` to non-nil past the early
/// return.
#[test]
fn truthy_unless_guard_narrows_the_rest_of_the_method() {
    let diags = check_fixture("truthy_early_return_catches_unknown_method.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0101"), "expected E0101, got: {diags:?}");
    assert!(diags[0].contains("nonexistent_method"), "got: {diags:?}");
}

/// SILENT: the false branch of a bare truthy check (`if x ... else
/// ... end`) must stay unrefined — Ruby's falsy set is `nil | false`, so
/// narrowing to `Nil` there could manufacture a diagnostic on an actual
/// `false` value.
#[test]
fn truthy_false_branch_has_no_narrowing_info() {
    let diags = check_fixture("truthy_false_branch_silent.rb");
    assert!(diags.is_empty(), "truthy false branch must stay silent, got: {diags:?}");
}

/// FIRES: ternary `x ? a : b` is an `IfNode` with a bare-variable
/// predicate under the hood — the true branch narrows `x` to non-nil the
/// same way `if x` does.
#[test]
fn ternary_truthy_narrows_the_then_branch() {
    let diags = check_fixture("ternary_truthy_catches_unknown_method.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0101"), "expected E0101, got: {diags:?}");
    assert!(diags[0].contains("nonexistent_method"), "got: {diags:?}");
}

/// FIRES: `return if x.nil? || <anything>` — the only way the whole
/// `||` is false is `x.nil?` itself being false, so `x` narrows to
/// non-nil past the early return even though the guard isn't a bare
/// `x.nil?` call.
#[test]
fn or_nil_guard_narrows_the_rest_of_the_method() {
    let diags = check_fixture("or_nil_guard_catches_unknown_method.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0101"), "expected E0101, got: {diags:?}");
    assert!(diags[0].contains("nonexistent_method"), "got: {diags:?}");
}

/// SILENT: `x.nil? || <anything>` being TRUE proves nothing about `x`
/// specifically (the `||` could hold purely because of the other side) —
/// narrowing `x` to `Nil` inside that branch would risk a false
/// diagnostic on a real method.
#[test]
fn or_nil_guard_true_branch_has_no_narrowing_info() {
    let diags = check_fixture("or_guard_true_branch_silent.rb");
    assert!(diags.is_empty(), "or-guard true branch must stay silent, got: {diags:?}");
}
