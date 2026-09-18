//! Bead ita-7g9: `def foo(...)` (Ruby 3.0 argument forwarding) parses in
//! prism as the method's `keyword_rest` param field holding a
//! `ForwardingParameterNode` — NOT the `rest` field the old arity
//! extraction only looked at (`crates/itaruby_semantic/src/index.rs`,
//! `DefWalker::method_def`). Every forwarding method was modeled with
//! arity `(required: 0, optional: 0, rest: false)`, so any call passing
//! args raised a false E0102 ("expects 0 arguments, got N") — 84
//! occurrences in a real-world corpus (found via ita-2dn), all on a
//! permission-predicate method that forwards its args.
//!
//! Fix: `method_def` now also sets `md.rest = true` when
//! `params.keyword_rest()` is a `ForwardingParameterNode`, matching
//! `*args`' shape — `check.rs::check_arity` already returns immediately
//! (no min, no max check) whenever `m.rest` is true, so forwarding
//! methods get fully open arity for free. `required`/`optional` are
//! untouched, so a real leading param before `...` (`def foo(a, ...)`)
//! still contributes its own required count.
//!
//! Fixtures live under `testdata/forwarding_args/` with a globally
//! unique `FwdArg` class-name prefix (`testdata/` is scanned as one
//! merged project by `ita check testdata/`, gate c).
//!
//! MUTANT THIS FILE MUST CATCH: reverting the `keyword_rest` /
//! `ForwardingParameterNode` disjunct (i.e. `md.rest =
//! params.rest().is_some()` alone, ignoring forwarding) flips
//! `pure_forwarding_silences_zero_one_three_args` from empty back to 3
//! E0102 diagnostics, while `no_forwarding_control_still_warns` stays
//! green — that control fixture has no `...` at all, so a mutant that
//! only touches the forwarding disjunct cannot move it.

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/forwarding_args");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, path.clone().into(), text);
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    itaruby_semantic::check_file(&db, file)
        .iter()
        .map(|d| {
            format!(
                "{}[{}]: {}",
                if d.severity == itaruby_semantic::Severity::Error {
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

/// G1: `def relay(...)` called with 0, 1, and 3 positional args must all
/// stay silent — forwarding has no cap and no minimum.
#[test]
fn pure_forwarding_silences_zero_one_three_args() {
    let diags = check_fixture("pure_forwarding_zero_one_three_args_silent.rb");
    assert!(diags.is_empty(), "expected silence, got: {diags:?}");
}

/// G1 variant: the mixed form (`def relay(tag, ...)`, Ruby 3.0) keeps its
/// real leading required param but still opens the max via the
/// forwarding tail.
#[test]
fn mixed_forwarding_leading_required_silences() {
    let diags = check_fixture("mixed_forwarding_leading_required_silent.rb");
    assert!(diags.is_empty(), "expected silence, got: {diags:?}");
}

/// G2 negative control: an ordinary method with NO `...` keeps its real,
/// fixed arity check — the forwarding fix must not widen unrelated
/// methods.
#[test]
fn no_forwarding_control_still_warns() {
    let diags = check_fixture("control_no_forwarding_wrong_arity_still_warns.rb");
    assert_eq!(diags.len(), 1, "expected exactly one arity diagnostic, got: {diags:?}");
    assert!(
        diags[0].contains("E0102") && diags[0].contains("expects 2 arguments, got 3"),
        "expected a real arity mismatch on FwdArgControlNoForwarding#relay, got: {diags:?}"
    );
}
