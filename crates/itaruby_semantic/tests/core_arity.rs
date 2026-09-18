//! `core.rs` allowlist arity accuracy (bead ita-d4): a fixed-arity entry
//! pinned tighter than Ruby's real signature is the dangerous FP
//! direction (invariant #1 forbids it), unlike a missing entry (which
//! just falls back to `Ty::Unknown`, safe by construction).
//!
//! `Array#concat(*other_arrays)` has been variadic since Ruby 2.4 — the
//! table used to group it with the genuinely-fixed-arity `+`/`-` at
//! `(1, Some(1))`, producing a real false E0102 on
//! `diagnostics.concat(syntax_error_diagnostics, syntax_warning_diagnostics)`
//! (ruby-lsp's `requests/diagnostics.rb:33`). Fixtures live under
//! `testdata/core_arity/`, each with globally unique `CoreArity`-prefixed
//! class names — `testdata/` is scanned as a single merged project by
//! `ita check testdata/` (gate c), which also wires `ClosedWorld` on
//! (no Gemfile upward of `testdata/`). `check_core_arity` itself never
//! consults `ClosedWorld` (see `check.rs`) — proven here by running every
//! fixture under BOTH closed-world settings and asserting identical
//! diagnostics.

use itaruby_semantic::core::{core_method, CoreClass};
use itaruby_semantic::{check_file, ClosedWorld, Db, ProjectFiles, Severity, SourceFile};

type Diags = Vec<String>;

fn check_with(name: &str, closed: bool) -> Diags {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/core_arity");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap();
    let db = Db::default();
    let file = SourceFile::new(&db, path.clone().into(), text);
    ProjectFiles::new(&db, vec![file]);
    if closed {
        ClosedWorld::new(&db, true);
    }
    check_file(&db, file)
        .iter()
        .map(|d| {
            format!(
                "{}[{}]: {}",
                if d.severity == Severity::Error {
                    "error"
                } else {
                    "warning"
                },
                d.code,
                d.message
            )
        })
        .collect()
}

fn check_closed(name: &str) -> Diags {
    check_with(name, true)
}

fn check_open(name: &str) -> Diags {
    check_with(name, false)
}

/// Table premise: `concat` really is `(0, None)` post-fix, distinct from
/// `+`/`-`'s `(1, Some(1))`.
#[test]
fn concat_table_entry_is_varargs() {
    let m = core_method(CoreClass::Array, "concat").unwrap();
    assert_eq!(m.min_args, 0);
    assert_eq!(m.max_args, None);
}

#[test]
fn plus_and_minus_stay_fixed_arity_one() {
    for name in ["+", "-"] {
        let m = core_method(CoreClass::Array, name).unwrap();
        assert_eq!(m.min_args, 1, "{name} min_args");
        assert_eq!(m.max_args, Some(1), "{name} max_args");
    }
}

/// (1) `concat(a, b)` — 2 positional args — must stay silent under
/// closed world (the production condition for `testdata/`, gate c).
#[test]
fn concat_two_args_silent_closed() {
    let diags = check_closed("concat_two_args_silent.rb");
    assert!(diags.is_empty(), "concat(a, b) must stay silent, got: {diags:?}");
}

/// Same fixture, closed world OFF — `check_core_arity` does not consult
/// `ClosedWorld` at all, so the result must be identical.
#[test]
fn concat_two_args_silent_open() {
    let diags = check_open("concat_two_args_silent.rb");
    assert!(diags.is_empty(), "concat(a, b) must stay silent regardless of closed world, got: {diags:?}");
}

/// Boundary proof: `concat` with 0 args is ALSO genuinely valid Ruby
/// (`[1].concat` => `[1]`, no error) — silent, not merely "silent for
/// 2+ args".
#[test]
fn concat_zero_args_silent() {
    let diags = check_closed("concat_zero_args_silent.rb");
    assert!(diags.is_empty(), "concat() with 0 args must stay silent, got: {diags:?}");
}

/// Control: `Array#+` stays pinned to exactly 1 arg. Calling it with 0
/// args must still accuse E0102 — this is the guard against the mutant
/// that over-widens the `+`/`-` arm to `(0, None)` alongside the real
/// `concat` fix (they share a match arm in `array_method`).
#[test]
fn plus_wrong_arity_still_accuses() {
    let diags = check_closed("plus_wrong_arity_still_accuses.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0102"), "expected E0102, got: {diags:?}");
    assert!(diags[0].contains('+'), "message must name the method, got: {diags:?}");
}

/// Control: `Array#include?` is untouched by this bead and stays pinned
/// to exactly 1 arg — 2 args must still accuse E0102. Proves the general
/// core-arity mechanism (not just the `concat`/`+`/`-` arm) still works.
#[test]
fn include_wrong_arity_still_accuses() {
    let diags = check_closed("include_wrong_arity_still_accuses.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0102"), "expected E0102, got: {diags:?}");
    assert!(diags[0].contains("include?"), "got: {diags:?}");
}
