//! Bead ita-w2c, bead C: "a call proven safe by a predicate".
//!
//! Two shapes, both fail-closed — each one narrows what is KNOWN, never
//! invents a type, and each is scoped to the branch/operand where the
//! proof actually holds:
//!
//! (i) `if respond_to?(:setup)` — the guard proves `self` answers that
//!     exact name, and only in the branch that runs when the predicate
//!     was TRUE (the `unless` else-clause is the mirror). Measured
//!     corpus site: `migrations/core/lib/migrations/conversion/base.rb:13-16`.
//! (ii) `x.is_a?(Class) && x < Base` — the left operand proves `x` is a
//!     class/module OBJECT, so the SECOND read of `x` is no longer the
//!     type the enclosing method gave it: it is `Ty::Unknown`, because no
//!     `Ty` models a singleton's method table. Measured corpus site:
//!     `actionpack/lib/action_dispatch/routing/endpoint.rb:14-16`.
//!
//! Fixtures live in `testdata/guard_narrowing/` and are MRI-executable:
//! every `*_silent` fixture really exits 0 and every `*_accuses` fixture
//! really raises on the diagnosed line (verified with ruby 3.4.2 — the
//! per-fixture line numbers asserted below are MRI's own backtrace lines).

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/guard_narrowing");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap();
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, path.clone().into(), text.clone());
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

/// Exactly one diagnostic, returned — the shape every control asserts:
/// its LINE (MRI's own raising line) and its code.
fn only(name: &str) -> String {
    let diags = check_fixture(name);
    assert_eq!(
        diags.len(),
        1,
        "{name} must be accused exactly once, got: {diags:?}"
    );
    diags[0].clone()
}

fn silent(name: &str) {
    let diags = check_fixture(name);
    assert!(diags.is_empty(), "{name} must be silent, got: {diags:?}");
}

/// SILENT: `setup` is called only inside `if respond_to?(:setup)`, the
/// exact proof that `self` answers it. MRI skips the branch and exits 0.
#[test]
fn respond_to_true_branch_is_silent() {
    silent("respond_to_true_branch_silent.rb");
}

/// SILENT: the same guard written `respond_to?(:setup, true)` — the
/// second argument widens which methods respond_to? counts, never what
/// the predicate proves.
#[test]
fn respond_to_second_argument_form_is_silent() {
    silent("respond_to_second_arg_silent.rb");
}

/// ACCUSED (line 14): the guard holds in the then-branch only, so the
/// same call in the ELSE-branch — which runs exactly when the method does
/// not exist — is a real missing method. MRI raises NameError at line 14.
#[test]
fn respond_to_else_branch_still_accuses() {
    let d = only("respond_to_else_branch_accuses.rb");
    assert!(d.starts_with("14:"), "expected the else-branch call, got: {d}");
    assert!(d.contains("E0101"), "expected E0101, got: {d}");
    assert!(
        d.contains("w2c_absent_setup"),
        "expected the unguarded name, got: {d}"
    );
}

/// ACCUSED (line 11): the guard is keyed on the exact name it names —
/// answering `w2c_present_step` proves nothing about `w2c_absent_step`.
/// MRI raises NameError at line 11.
#[test]
fn respond_to_other_name_still_accuses() {
    let d = only("respond_to_wrong_name_accuses.rb");
    assert!(d.starts_with("11:"), "expected the unguarded call, got: {d}");
    assert!(d.contains("E0101"), "expected E0101, got: {d}");
    assert!(
        d.contains("w2c_absent_step"),
        "expected the differently-named call, got: {d}"
    );
}

/// ACCUSED (line 8): the same absent self-send with no guard at all.
/// MRI raises NameError at line 8.
#[test]
fn respond_to_unguarded_self_send_still_accuses() {
    let d = only("respond_to_unguarded_accuses.rb");
    assert!(d.starts_with("8:"), "expected the bare call, got: {d}");
    assert!(d.contains("E0101"), "expected E0101, got: {d}");
}

/// SILENT: `rack_app.is_a?(Class) && rack_app < Base` — the second read
/// of `rack_app` is a class object, whose `<` no `Ty` can vouch for.
/// MRI: `rack_app` is the endpoint itself, so the left operand is false
/// and `&&` short-circuits to false, exit 0.
#[test]
fn class_object_guard_on_a_receiverless_call_is_silent() {
    silent("class_object_receiver_call_silent.rb");
}

/// SILENT: the same proof on a LOCAL variable — `endpoint` stops being
/// `Instance(Endpoint)` for the right operand. MRI short-circuits to
/// false, exit 0.
#[test]
fn class_object_guard_on_a_local_is_silent() {
    silent("class_object_local_silent.rb");
}

/// ACCUSED (line 20): `rack_app < Base` with no `is_a?(Class)` in front
/// of it proves nothing. MRI raises NoMethodError at line 20.
#[test]
fn class_object_unguarded_comparison_still_accuses() {
    let d = only("class_object_unguarded_accuses.rb");
    assert!(d.starts_with("20:"), "expected the bare comparison, got: {d}");
    assert!(d.contains("E0101"), "expected E0101, got: {d}");
    assert!(d.contains("`<`"), "expected the `<` call named, got: {d}");
}

/// ACCUSED (line 22): the fact attaches to the SUBJECT of the left
/// operand, never to the `&&` as a whole — a different receiver in the
/// right operand keeps its own type. MRI raises NoMethodError at line 22.
#[test]
fn class_object_guard_does_not_cover_another_subject() {
    let d = only("class_object_other_subject_accuses.rb");
    assert!(d.starts_with("22:"), "expected the other receiver, got: {d}");
    assert!(d.contains("E0101"), "expected E0101, got: {d}");
    assert!(d.contains("::Other"), "expected Other named, got: {d}");
}

/// ACCUSED (line 14): `respond_to?` is a statement about the receiver it
/// is sent to (`self`), never about the receiver of the guarded call.
/// MRI: `self` answers the name, the branch runs, and `Helper` does not.
#[test]
fn respond_to_guard_does_not_cover_an_explicit_receiver() {
    let d = only("respond_to_other_receiver_accuses.rb");
    assert!(d.starts_with("14:"), "expected the other receiver, got: {d}");
    assert!(d.contains("E0101"), "expected E0101, got: {d}");
    assert!(d.contains("::Helper"), "expected Helper named, got: {d}");
}

/// SILENT: the `Module` mirror of the class-object proof. MRI
/// short-circuits to false, exit 0.
#[test]
fn class_object_guard_on_is_a_module_is_silent() {
    silent("class_object_module_guard_silent.rb");
}

/// ACCUSED (line 16): a project constant named `Class` shadows the core
/// one lexically, so the guard's evidence is gone and the comparison is
/// checked normally. MRI raises NoMethodError at line 16.
#[test]
fn class_object_guard_bails_on_a_shadowed_class_constant() {
    let d = only("class_object_shadowed_const_accuses.rb");
    assert!(d.starts_with("16:"), "expected the comparison, got: {d}");
    assert!(d.contains("E0101"), "expected E0101, got: {d}");
    assert!(d.contains("`<`"), "expected the `<` call named, got: {d}");
}

/// ACCUSED (line 14): the guard speaks only about class/module OBJECTS.
/// `thing.is_a?(Base)` — an ordinary project class the checker models
/// fully — must not silence anything. MRI raises NoMethodError at line 14.
#[test]
fn class_object_guard_does_not_fire_on_a_project_class() {
    let d = only("class_object_project_class_accuses.rb");
    assert!(d.starts_with("14:"), "expected the comparison, got: {d}");
    assert!(d.contains("E0101"), "expected E0101, got: {d}");
    assert!(d.contains("::Base"), "expected Base named, got: {d}");
}