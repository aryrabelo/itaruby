//! Flow-sensitive narrowing (bead ita-u1t): `if x.is_a?(Foo)` / `if x.nil?`
//! / `unless x.nil?` / `return if x.nil?` / `raise ... if x.nil?` for local
//! variables and method parameters. Ivar narrowing is explicitly out of
//! scope (contract): a method call can mutate an ivar at any point, so
//! ivars only ever get an unrefined type (see `ivar_types.rs`), never a
//! flow-narrowed one. Fixtures live under `testdata/narrowing/`, each with
//! globally unique class names — `testdata/` is scanned as a single merged
//! project by `ita check testdata/` (gate c), so a name collision with any
//! other fixture in the tree would leak diagnostics across files.

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/narrowing");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap();
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, path.clone().into(), text);
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    itaruby_semantic::check_file(&db, file)
        .iter()
        .map(|d| format!("{}[{}]: {}", if d.severity == itaruby_semantic::Severity::Error { "error" } else { "warning" }, d.code, d.message))
        .collect()
}

/// FIRES: inside `if x.is_a?(Foo)`, `x` narrows to `Foo` — an unknown
/// method on it is a real E0101.
#[test]
fn is_a_then_branch_narrows_and_catches_unknown_method() {
    let diags = check_fixture("is_a_then_unknown_method.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0101"), "expected E0101, got: {diags:?}");
    assert!(diags[0].contains("nonexistent_method"), "got: {diags:?}");
}

/// SILENT: the `else` of `x.is_a?(Foo)` proves nothing about `x` — it
/// must stay unrefined, not be narrowed to some other guessed type.
#[test]
fn is_a_else_branch_has_no_narrowing_info() {
    let diags = check_fixture("is_a_else_silent.rb");
    assert!(diags.is_empty(), "is_a? else branch must stay silent, got: {diags:?}");
}

/// SILENT: reassigning the narrowed variable inside the branch kills the
/// narrowing immediately — a call that would only be safe under the old
/// (killed) narrowing must not fire.
#[test]
fn reassignment_inside_branch_kills_narrowing() {
    let diags = check_fixture("reassignment_kills_narrow.rb");
    assert!(diags.is_empty(), "reassignment must kill narrowing silently, got: {diags:?}");
}

/// SILENT: narrowing does not leak past the end of the `if` block when
/// there's no early return — usage outside the branch reverts to the
/// pre-if (unrefined) type.
#[test]
fn narrowing_does_not_leak_outside_the_branch() {
    let diags = check_fixture("outside_branch_silent.rb");
    assert!(diags.is_empty(), "narrowing must not leak past the if, got: {diags:?}");
}

/// FIRES (bead ita-u1t contract: "return if x.nil? / raise ... if
/// x.nil? narrows the rest of the method after the early return"): both the
/// `return`-guard and `raise`-guard idioms must narrow `x` to non-nil for
/// the rest of the method.
#[test]
fn early_return_and_raise_nil_guards_narrow_the_rest_of_the_method() {
    let diags = check_fixture("early_return_nil_check.rb");
    assert_eq!(diags.len(), 2, "expected 2 diagnostics (one per method), got: {diags:?}");
    assert!(diags.iter().all(|d| d.contains("E0101") && d.contains("nonexistent_method")), "got: {diags:?}");
}
