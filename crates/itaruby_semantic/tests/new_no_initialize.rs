//! Bead ita-gjb: `X.new(args)` on a class whose `initialize` is
//! `MethodLookup::NotFound` (fully closed ancestry, no `initialize`
//! anywhere in it) used to assume `Object#initialize`'s 0-arg default and
//! flag any argument as E0102. That is unsound in general — this checker
//! never models the C-level constructors of classes it has no inventory
//! for (the historical FPs were core classes like `Time`/`IPAddr` and gems
//! like `FastImage`, most of which take arguments in native code), and a
//! "fully closed" ancestry per this checker's own model is not proof the
//! real runtime `initialize` is nowhere else either. Same root as ita-h6l
//! mechanism A/B (silence over a fabricated diagnostic), generalized from
//! `Class`/`Struct`'s exact-name carve-out to every `NotFound` verdict on
//! `initialize`.
//!
//! Fixtures live under `testdata/new_no_initialize/` with a `NoInit`
//! class-name prefix — `testdata/` is scanned as one merged project by
//! `ita check testdata/` (gate c), so names must stay globally unique.
//!
//! MUTANT THIS FILE MUST CATCH: reintroducing the old `NotFound` arm
//! (arity check against 0 when `!pos_args.is_empty()`) flips
//! `silent_no_initialize_anywhere_produces_no_diagnostic` from empty to a
//! single E0102, while every control test below stays green — none of
//! them exercise the `NotFound` arm at all (their `initialize` is always
//! `Found`), so a mutant that only touches `NotFound` cannot move them.

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/new_no_initialize");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, path.clone().into(), text);
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    itaruby_semantic::check_file(&db, file)
        .iter()
        .map(|d| format!("{}[{}]: {}", if d.severity == itaruby_semantic::Severity::Error { "error" } else { "warning" }, d.code, d.message))
        .collect()
}

/// G1: a class with no `initialize` anywhere in a fully-closed ancestry
/// must never manufacture an E0102 from an assumed 0-arg constructor.
#[test]
fn silent_no_initialize_anywhere_produces_no_diagnostic() {
    let diags = check_fixture("silent_no_initialize_anywhere.rb");
    assert!(diags.is_empty(), "expected silence, got: {diags:?}");
}

/// G2 control A: the receiver's OWN `initialize` (arity 0) is a real
/// `MethodLookup::Found` — calling it with arguments still raises E0102.
#[test]
fn control_own_zero_arity_initialize_still_warns() {
    let diags = check_fixture("control_own_zero_arity_initialize_still_warns.rb");
    assert_eq!(diags.len(), 1, "expected exactly one arity diagnostic, got: {diags:?}");
    assert!(
        diags[0].contains("E0102") && diags[0].contains("expects 0 arguments, got 1"),
        "expected a real 0-arity mismatch, got: {diags:?}"
    );
}

/// G2 control B: the other arity direction — `initialize(x)` (arity 1)
/// called with zero arguments still raises E0102.
#[test]
fn control_own_one_arity_initialize_still_warns() {
    let diags = check_fixture("control_own_one_arity_initialize_still_warns.rb");
    assert_eq!(diags.len(), 1, "expected exactly one arity diagnostic, got: {diags:?}");
    assert!(
        diags[0].contains("E0102") && diags[0].contains("expects 1 argument, got 0"),
        "expected a real 1-arity mismatch, got: {diags:?}"
    );
}

/// G2 control C: `initialize` inherited from a PROJECT ancestor (not the
/// receiver class itself) is still `Found` via the ancestor walk — arity
/// checking survives inheritance, only a true `NotFound` is silenced.
#[test]
fn control_inherited_initialize_still_warns() {
    let diags = check_fixture("control_inherited_initialize_still_warns.rb");
    assert_eq!(diags.len(), 1, "expected exactly one arity diagnostic, got: {diags:?}");
    assert!(
        diags[0].contains("E0102") && diags[0].contains("expects 1 argument, got 0"),
        "expected a real 1-arity mismatch via inheritance, got: {diags:?}"
    );
}
