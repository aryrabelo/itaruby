//! Instance-variable type inference (bead ita-u1t): the union of every
//! `@name = <expr>` assignment across a class's own instance methods.
//! Deliberately narrow: two different assignment types (or any Unknown
//! assignment, or no visible assignment at all) collapse straight to
//! `Ty::Unknown` — see `Checker::ivar_ty` in `check.rs` for why this is
//! not `Ty::union`. Ivar *narrowing* (flow-sensitive refinement) is out of
//! scope by contract; only the type itself is inferred here. Fixtures
//! live under `testdata/ivar_types/`, each with globally unique class
//! names — `testdata/` is scanned as a single merged project by `ita
//! check testdata/` (gate c), so a name collision with any other fixture
//! in the tree would leak diagnostics across files.

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/ivar_types");
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

/// FIRES: a single `@gadget = Gadget.new` assignment in `initialize`
/// types `@gadget` as `Gadget` — an unknown method on it is a real E0101.
#[test]
fn single_assignment_types_the_ivar_and_catches_unknown_method() {
    let diags = check_fixture("ivar_unknown_method.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0101"), "expected E0101, got: {diags:?}");
    assert!(diags[0].contains("nonexistent_method"), "got: {diags:?}");
}

/// SILENT: two assignments of different types (`Gadget` vs `String`)
/// collapse the ivar's type to Unknown — never a false positive.
#[test]
fn two_different_assignment_types_stay_silent() {
    let diags = check_fixture("ivar_union_mismatch_silent.rb");
    assert!(diags.is_empty(), "mismatched ivar assignment types must stay silent, got: {diags:?}");
}

/// SILENT: an ivar read with no visible assignment anywhere in the class
/// body is Unknown, never an error.
#[test]
fn no_visible_assignment_stays_silent() {
    let diags = check_fixture("ivar_no_assignment_silent.rb");
    assert!(diags.is_empty(), "ivar with no assignment must stay silent, got: {diags:?}");
}
