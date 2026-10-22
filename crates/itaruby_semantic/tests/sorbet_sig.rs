//! Index-blind type-expression mapping (`sorbet_ret_ty`) and the
//! class-openness controls around an unrecognized `sig` block. The named
//! contracts themselves are exercised end to end, through real
//! diagnostics, in `sorbet_contracts.rs`.

use itaruby_semantic::index::{parse_defs_text, ClassFragment, OpenReason};
use itaruby_semantic::sorbet_sig::sorbet_ret_ty;
use itaruby_semantic::types::Ty;

// ---------------------------------------------------------------------------
// sorbet_ret_ty: one case per Contract table row, plus nesting.
// ---------------------------------------------------------------------------

#[test]
fn maps_integer() {
    assert_eq!(sorbet_ret_ty("Integer"), Ty::Int);
}

#[test]
fn maps_float() {
    assert_eq!(sorbet_ret_ty("Float"), Ty::Float);
}

#[test]
fn maps_string() {
    assert_eq!(sorbet_ret_ty("String"), Ty::Str);
}

#[test]
fn maps_symbol() {
    assert_eq!(sorbet_ret_ty("Symbol"), Ty::Sym);
}

#[test]
fn maps_t_boolean_and_true_false_class() {
    assert_eq!(sorbet_ret_ty("T::Boolean"), Ty::Bool);
    assert_eq!(sorbet_ret_ty("TrueClass"), Ty::Bool);
    assert_eq!(sorbet_ret_ty("FalseClass"), Ty::Bool);
}

#[test]
fn maps_nil_class() {
    assert_eq!(sorbet_ret_ty("NilClass"), Ty::Nil);
}

#[test]
fn maps_t_nilable() {
    assert_eq!(sorbet_ret_ty("T.nilable(Integer)"), Ty::union(Ty::Int, Ty::Nil));
}

#[test]
fn maps_t_array() {
    assert_eq!(sorbet_ret_ty("T::Array[String]"), Ty::Array(Box::new(Ty::Str)));
}

#[test]
fn maps_t_hash() {
    assert_eq!(
        sorbet_ret_ty("T::Hash[String, Integer]"),
        Ty::Hash(Box::new(Ty::Str), Box::new(Ty::Int))
    );
}

#[test]
fn maps_t_any() {
    assert_eq!(sorbet_ret_ty("T.any(Integer, String)"), Ty::union(Ty::Int, Ty::Str));
}

/// Aninhamento: `T.nilable(T::Array[String])` -> `Union([Array(Str), Nil])`.
#[test]
fn nested_nilable_array() {
    assert_eq!(
        sorbet_ret_ty("T.nilable(T::Array[String])"),
        Ty::Union(vec![Ty::Array(Box::new(Ty::Str)), Ty::Nil])
    );
}

/// `T::Hash[Symbol, T.nilable(Integer)]`: the top-level comma split must
/// land at depth 0 only — three `,`/`(`/`)` tokens inside, exactly two
/// hash type args.
#[test]
fn nested_hash_with_nilable_value() {
    assert_eq!(
        sorbet_ret_ty("T::Hash[Symbol, T.nilable(Integer)]"),
        Ty::Hash(Box::new(Ty::Sym), Box::new(Ty::union(Ty::Int, Ty::Nil)))
    );
}

/// Whitespace and a leading `::` inside a wrapper's brackets.
#[test]
fn tolerates_whitespace_and_leading_colon_colon() {
    assert_eq!(sorbet_ret_ty("::Integer"), Ty::Int);
    assert_eq!(sorbet_ret_ty("T::Array[ ::String ]"), Ty::Array(Box::new(Ty::Str)));
}

/// Three unrecognized forms, each `Unknown` — never a chute.
#[test]
fn unrecognized_forms_are_unknown() {
    // A gem class name written as a return type.
    assert_eq!(sorbet_ret_ty("ActiveRecord::Relation"), Ty::Unknown);
    // `T.proc` (callback type) — not modeled.
    assert_eq!(
        sorbet_ret_ty("T.proc.params(x: Integer).returns(String)"),
        Ty::Unknown
    );
    // `T::Set[...]` — not modeled (only `T::Array`/`T::Hash` are).
    assert_eq!(sorbet_ret_ty("T::Set[String]"), Ty::Unknown);
}

/// The central safety property (Contract): a name that looks exactly like
/// a project class must map to `Unknown`, NEVER `Instance`/`Class` — this
/// function has no `ProjectIndex`, so it has no `ClassId` to attach to any
/// name, project or gem.
#[test]
fn never_produces_instance_or_class() {
    assert_eq!(sorbet_ret_ty("Widget"), Ty::Unknown);
    assert_eq!(sorbet_ret_ty("SomeApp::Models::Order"), Ty::Unknown);
    match sorbet_ret_ty("Widget") {
        Ty::Instance(_) | Ty::Class(_) => panic!("sorbet_ret_ty must never produce Instance/Class"),
        _ => {}
    }
}

fn widget_fragment(text: &str) -> ClassFragment {
    parse_defs_text(text)
        .fragments
        .into_iter()
        .find(|f| f.path == "Widget")
        .expect("fragment `Widget` not found")
}


/// Bead ita-4xy narrows this: a RECOGNIZED `sig { ... }` shape (bare
/// `returns(X)`/`void`, optionally `params(...)`/`override.`/`abstract.`
/// chained — `sig_block_is_recognized`'s exact grammar) no longer opens
/// the class it sits in — pure Sorbet type metadata, never a method
/// definition, same reasoning as the existing `define_method`-with-a-
/// literal-name carve-out just above it. An UNRECOGNIZED shape (multi-
/// statement block, or an outermost call that's neither `returns` nor
/// `void`) still opens with the SAME `ClassBodyBlock` reason as every
/// other unmodeled block-taking call — `tests/open_reason.rs`'s
/// `class_body_block` test pins the identical reason for the
/// `ActiveSupport::Concern` `included do ... end` idiom.
#[test]
fn recognized_sig_no_longer_opens_the_class() {
    let frag = widget_fragment(
        "class Widget\n  sig { returns(String) }\n  def name\n  end\nend\n",
    );
    assert!(
        !frag.open,
        "a RECOGNIZED sig {{ ... }} must no longer open the fragment (bead ita-4xy)"
    );
    assert_eq!(frag.open_reason, None);
}

#[test]
fn unrecognized_sig_shape_still_opens_the_class() {
    let frag = widget_fragment(
        "class Widget\n  sig { some_unmodeled_call(X) }\n  def name\n  end\nend\n",
    );
    assert!(
        frag.open,
        "an UNRECOGNIZED sig {{ ... }} shape must still open the fragment"
    );
    assert_eq!(frag.open_reason, Some(OpenReason::ClassBodyBlock));
}

#[test]
fn sig_void_no_longer_opens_the_class() {
    let frag = widget_fragment("class Widget\n  sig { void }\n  def save\n  end\nend\n");
    assert!(
        !frag.open,
        "`sig {{ void }}` is a recognized shape too, must not open the fragment"
    );
}
