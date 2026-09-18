//! Bead ita-h6l, mechanism A: `Class.new(Superclass)` / `Struct.new(:a, :b)`
//! build and return a brand-new ANONYMOUS class/struct, never an instance of
//! the receiver. `check.rs`'s `Ty::Class(c)` "new" arm used to look up
//! `initialize` and return `Ty::Instance(c)` unconditionally — correct for
//! `MyModel.new`, wrong for these two: the dominant FP family measured
//! against rails/rails (68.9% of the 3131-error baseline), because
//! `Class`/`Struct` reach this arm only once the project itself reopens
//! them (Rails' own `active_support/core_ext/class/*.rb` and
//! `core_ext/struct.rb` do exactly that), and the wrong `initialize` lookup
//! plus the wrong return type cascade into false E0101/E0102.
//!
//! MUTANTS THIS FILE MUST CATCH:
//!   1. The carve-out removed entirely (the `if path == "Class" || path ==
//!      "Struct" { ...; return Ty::Unknown; }` block deleted from
//!      `Checker::infer_expr_inner`'s `Ty::Class(c)` arm) —
//!      `class_new_infers_as_unknown`/`struct_new_infers_as_unknown` flip
//!      from `None` to `Some("Class")`/`Some("Struct")`: this is the only
//!      assertion in the file that is NOT also masked by mechanism B (see
//!      below), because the wrong RETURN TYPE is untouched by ancestry
//!      open/closed status.
//!   2. Only one arm of the `||` survives (e.g. `path == "Class"` alone) —
//!      caught by whichever of the two single-name tests lost its carve-out.
//!   3. The carve-out broadened past an exact match (e.g. any capitalized
//!      receiver, or a `starts_with` check) — `ordinary_class_new_still_
//!      checks_arity` flips from an E0102 diagnostic to silence.
//!
//! Note on mechanism B interaction: `class Class ... end` / `class Struct
//! ... end` is ALSO a reopening of a known core name (bead ita-h6l,
//! mechanism B), which independently marks the fragment `open`. Once open,
//! `MethodLookup` on it is always `Inconclusive`, never `Found`/`NotFound`
//! — so the ARITY diagnostic assertions below hold even without mechanism
//! A. Kept anyway because they pin the literal acceptance criterion
//! ("no arity diagnostic") end-to-end, and because the hover
//! assertions prove mechanism A's OWN, non-redundant contribution: the
//! return type.

use itaruby_semantic::{check_file, hover_at, Db, ProjectFiles, SourceFile};

fn diags_of(text: &str) -> Vec<String> {
    let db = Db::default();
    let file = SourceFile::new(&db, "inline/anon_class_new.rb".into(), text.to_string());
    ProjectFiles::new(&db, vec![file]);
    check_file(&db, file)
        .iter()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect()
}

fn ty_at(text: &str, needle: &str) -> Option<String> {
    let db = Db::default();
    let file = SourceFile::new(&db, "inline/anon_class_new_hover.rb".into(), text.to_string());
    ProjectFiles::new(&db, vec![file]);
    let offset = text
        .find(needle)
        .unwrap_or_else(|| panic!("needle {needle:?} not found in fixture"));
    hover_at(&db, file, offset).and_then(|h| h.ty)
}

/// `Class` reopened with a real (mismatched-arity) `initialize` — real
/// Rails shape (`active_support/core_ext/class/attribute.rb` reopens
/// `Class` without ever touching `initialize`, but the project reopening
/// alone is enough to intern the receiver as `Ty::Class`). The call site
/// must resolve to `Ty::Unknown`, never `Ty::Instance(Class)`.
#[test]
fn class_new_infers_as_unknown() {
    let src = "class Class\n  def initialize(a, b)\n  end\nend\n\nclass H6lApplicant\nend\n\nClass.new(H6lApplicant)\n";
    assert_eq!(
        ty_at(src, "new(H6lApplicant)"),
        None,
        "Class.new(...) must infer Ty::Unknown, never Ty::Instance(Class)"
    );
}

/// Same proof for `Struct` — the other half of the `||`. Real Rails shape:
/// `active_support/core_ext/struct.rb` adds `Struct#to_h`.
#[test]
fn struct_new_infers_as_unknown() {
    let src = "class Struct\n  def to_h(hash_class: Hash)\n  end\nend\n\nStruct.new(:a, :b)\n";
    assert_eq!(
        ty_at(src, "new(:a, :b)"),
        None,
        "Struct.new(...) must infer Ty::Unknown, never Ty::Instance(Struct)"
    );
}

/// The literal acceptance criterion: zero arity diagnostics on
/// `Class.new(Superclass)` even when `Class`'s own (mismatched-arity)
/// `initialize` is reopened right there in the project.
#[test]
fn class_new_with_superclass_arg_raises_no_arity_diagnostic() {
    let src = "class Class\n  def initialize(a, b)\n  end\nend\n\nclass H6lApplicant\nend\n\nClass.new(H6lApplicant)\n";
    let diags = diags_of(src);
    assert!(diags.is_empty(), "expected silence, got: {diags:?}");
}

/// Same for `Struct.new(:a, :b)` — variadic, no fixed arity to check
/// against at all.
#[test]
fn struct_new_with_symbols_raises_no_arity_diagnostic() {
    let src = "class Struct\n  def to_h(hash_class: Hash)\n  end\nend\n\nStruct.new(:a, :b)\n";
    let diags = diags_of(src);
    assert!(diags.is_empty(), "expected silence, got: {diags:?}");
}

/// Mutant 3's counter-proof: an ORDINARY project class named neither
/// `Class` nor `Struct` must keep its real arity check. If the carve-out
/// ever broadens past an exact-name match, this is the test that catches
/// it — `H6lWidget` is a plain, unrelated, single-capitalized-word class,
/// exactly the shape a sloppy "any Class-typed receiver" mutant would
/// wrongly swallow.
#[test]
fn ordinary_class_new_still_checks_arity() {
    let src = "class H6lWidget\n  def initialize(a)\n  end\nend\n\nH6lWidget.new(1, 2)\n";
    let diags = diags_of(src);
    assert_eq!(diags.len(), 1, "expected exactly one arity diagnostic, got: {diags:?}");
    assert!(
        diags[0].contains("E0102") && diags[0].contains("expects 1 argument, got 2"),
        "expected a real arity mismatch on H6lWidget.new, got: {diags:?}"
    );
}

/// And its hover counterpart: an ordinary class's `.new` still infers as
/// `Instance(H6lWidget)`, proving the carve-out truly is name-scoped and
/// not a blanket "every `.new` call is Unknown" regression.
#[test]
fn ordinary_class_new_still_infers_instance() {
    let src = "class H6lGadget\n  def initialize(a)\n  end\nend\n\nH6lGadget.new(1)\n";
    assert_eq!(ty_at(src, "new(1)"), Some("H6lGadget".to_string()));
}
