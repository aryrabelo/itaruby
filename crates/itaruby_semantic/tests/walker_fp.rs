//! `WalkerFP` (beads ita-o8l.4 + ita-o8l.5): two independent false-positive
//! mechanisms, both suppression-only (invariant #1 — `Ty::Unknown` never
//! produces a diagnostic).
//!
//! (1) `.extend(Module)` widening on an ivar receiver (bead ita-o8l.4):
//! `check_call`'s existing local-variable widening (`env.insert(var,
//! Ty::Unknown)`) cannot apply to an ivar — ivar narrowing is out of scope
//! by contract (any method can reassign an ivar). `ivar_ty` is already
//! class-wide, not per-method, so the widening instead poisons the SAME
//! `ivar_capture` side channel `InstanceVariableWriteNode` writes use,
//! surviving across method boundaries by construction — exactly the real
//! corpus shape (`extend` in `setup`, the read in a separate test method).
//! Deliberately NOT gated on `closed_world()` the way the local-variable
//! case is: widening to Unknown can never fabricate a diagnostic regardless
//! of world-openness, and the real corpus (a `Gemfile` with no complete
//! Tapioca coverage) measures `closed_world()` false, so a gated version
//! would never fire at all.
//!
//! (2) A `def` inside a class-body `begin`/`rescue`/`else`/`ensure` was
//! never indexed by `DefWalker`, while the sibling `if`/`else` arm already
//! was (bead ita-o8l.5) — `walk_begin_arms` now walks every arm the same
//! "conservative: walk every arm" way, since which arm actually executes
//! is unknowable statically and an indexed name can only turn an existing
//! `NotFound` off, never fabricate a new one.
//!
//! Fixtures live under `testdata/walker_fp/`, each with globally unique
//! `WalkerFp`-prefixed class/module names — `testdata/` is scanned as one
//! merged project by `ita check testdata/`, so a name collision with any
//! other fixture in the tree would leak diagnostics across files.

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/walker_fp");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap();
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, path.clone().into(), text);
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    itaruby_semantic::check_file(&db, file)
        .iter()
        .map(|d| {
            format!(
                "{}[{}]: {}",
                if d.severity == itaruby_semantic::Severity::Error { "error" } else { "warning" },
                d.code,
                d.message
            )
        })
        .collect()
}

/// SILENT: `@target.extend WalkerFpIvarExtendHelper` in `setup`, then
/// `@target.walker_fp_extended_method` in a DIFFERENT method (`test_it`) —
/// the widening must survive the method boundary.
#[test]
fn walker_fp_ivar_extend_silences_cross_method_read() {
    let diags = check_fixture("ivar_extend_silences_cross_method.rb");
    assert!(diags.is_empty(), "ivar extend must silence the cross-method read, got: {diags:?}");
}

/// FIRES (negative control): no `.extend` anywhere on this ivar, so the
/// widening must never fire and the genuinely unknown method still accuses.
#[test]
fn walker_fp_ivar_without_extend_still_accuses() {
    let diags = check_fixture("ivar_no_extend_still_accuses.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0101"), "expected E0101, got: {diags:?}");
    assert!(diags[0].contains("walker_fp_never_defined_anywhere"), "got: {diags:?}");
}

/// SILENT: a `def` in `begin`, `rescue`, `else`, and `ensure` all resolve —
/// one distinct method name per arm, so each arm's own coverage is
/// independently provable.
#[test]
fn walker_fp_begin_rescue_else_ensure_arms_all_resolve() {
    let diags = check_fixture("begin_rescue_arms_resolve.rb");
    assert!(diags.is_empty(), "def in every begin/rescue/else/ensure arm must resolve, got: {diags:?}");
}

/// FIRES (negative control): a method defined in NEITHER the `begin` nor
/// the `rescue` arm must still accuse — proves the fix walks exactly the
/// arms present, never opens the whole class as a side effect.
#[test]
fn walker_fp_method_in_neither_begin_nor_rescue_arm_still_accuses() {
    let diags = check_fixture("begin_rescue_neither_arm_accuses.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0101"), "expected E0101, got: {diags:?}");
    assert!(
        diags[0].contains("walker_fp_totally_undefined_neither_arm"),
        "got: {diags:?}"
    );
}
