//! Bead ita-s12: a generic-method type parameter declared by an RBS sig
//! comment's leading `[T]` list (`#: [T] (String, T) -> T`) was being
//! resolved through `rbs_to_ty`'s ordinary `RbsTy::Simple` fallback —
//! treating the type VARIABLE `T` as if it named a real project/core
//! class. Measured false E0103 (third sighting: Round-4 residual +
//! `cache_set` + ruby-lsp's `erb_document_test.rb:161`): ruby-lsp vendors
//! a real `T` module (sorbet-runtime's RBI stub), so `Document#cache_set`'s
//! `#: [T] (String, T) -> T` rejected an `Array` argument passed for `T`.
//!
//! Fix: `RbsSig::type_params` now captures the names declared by `[T,
//! ...]`; `check.rs::rbs_to_ty` binds every one of them to `Ty::Unknown`
//! for the scope of THAT sig — sound by construction (invariant #1:
//! `Unknown` never diagnoses). Fixtures live under
//! `testdata/generic_type_params/`, each with a globally unique
//! `GenT`-prefixed class name (`testdata/` is scanned as one merged
//! project by `ita check testdata/`, gate c) — three of the four also
//! locally declare an empty `class T; end` marker (mirroring ruby-lsp's
//! own vendored `T`, the exact shape that exposed the bug); all three
//! declarations are identical no-op reopens, so merging them in gate c's
//! single project is harmless.
//!
//! MUTANTS THIS FILE MUST CATCH:
//! (a) binding removed — delete the
//!     `RbsTy::Simple(name) if type_params.iter().any(|p| p == name) =>
//!     Ty::Unknown,` arm from `check.rs::rbs_to_ty`. Because
//!     `sig_type_param_accepts_any_arg_silent.rb` itself declares `class
//!     T; end`, the mutant makes `T` resolve to that real nominal class
//!     instead of Unknown, so the `Array[Integer]` argument no longer
//!     matches — `sig_type_param_accepts_any_arg_silent_no_diagnostics`
//!     starts reporting 1 diagnostic instead of 0.
//! (b) scope leak — in `check.rs::check_method_body`, replace
//!     `let type_params: &[String] = sig.map(|s| s.type_params.as_slice()).unwrap_or(&[]);`
//!     with `let type_params: &[String] = &["T".to_string()];` (simulating
//!     `T` bleeding into every method's own param/return resolution
//!     regardless of whether THAT method's sig ever declared `[T]`) — and
//!     `nominal_call_after_generic_sibling_still_accuses` (driven off
//!     `generic_then_nominal_no_scope_leak.rb`'s SECOND call, whose sig
//!     has no `[T]` list at all) drops from 1 diagnostic to 0.

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/generic_type_params");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap();
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, path.clone().into(), text);
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    itaruby_semantic::ClosedWorld::new(&db, true);
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

/// SILENT: `#: [T] (String, T) -> T` called with `(String, Array[Integer])`
/// — `T` accepts any type, must not fire E0103.
#[test]
fn sig_type_param_accepts_any_arg_silent_no_diagnostics() {
    let diags = check_fixture("sig_type_param_accepts_any_arg_silent.rb");
    assert_eq!(diags, Vec::<String>::new(), "T must accept any argument type, got: {diags:?}");
}

/// FIRES: same `[T]` sig, but the mismatch is on the sig's own concrete
/// `String` parameter (position 1) — binding `T` to Unknown must never
/// widen an unrelated, concretely-typed parameter's check.
#[test]
fn wrong_concrete_arg_still_accuses_even_with_generic_sig() {
    let diags = check_fixture("sig_type_param_wrong_first_arg_still_accuses.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0103"), "expected E0103, got: {diags:?}");
    assert!(diags[0].contains("argument 1"), "expected the mismatch on argument 1, got: {diags:?}");
}

/// FIRES (precedence pin): a sig with NO `[T, ...]` list that mentions a
/// real project class literally named `T` must still resolve `T` as that
/// nominal class — the type-param binding only applies inside a sig that
/// actually declares `[T]`.
#[test]
fn nominal_t_class_without_generic_list_still_accuses() {
    let diags = check_fixture("nominal_t_without_generic_list_still_accuses.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0103"), "expected E0103, got: {diags:?}");
}

/// FIRES (scope-leak control, mutant b): one class declares a `[T]`
/// generic method AND a plain nominal-`T` method. Checking the generic
/// method's sig first must never leave `T` bound to Unknown for the
/// SECOND method's sig — its nominal-`T` mismatch must still accuse.
#[test]
fn nominal_call_after_generic_sibling_still_accuses() {
    let diags = check_fixture("generic_then_nominal_no_scope_leak.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic (the nominal call), got: {diags:?}");
    assert!(diags[0].contains("E0103"), "expected E0103, got: {diags:?}");
}
