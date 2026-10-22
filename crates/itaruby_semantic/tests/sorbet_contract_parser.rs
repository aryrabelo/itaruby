//! Boundary controls for static signature extraction and lexical type mapping.
//! These do not execute Ruby or depend on external RBI files.

use itaruby_semantic::index::ProjectIndex;
use itaruby_semantic::sorbet_sig::{extract_sig, resolve_sig_ty, sorbet_ret_ty, SorbetSig};
use itaruby_semantic::types::Ty;
use itaruby_semantic::{Db, ProjectFiles, SourceFile};

fn signature(source: &str) -> Option<SorbetSig> {
    let parsed = ruby_prism::parse(source.as_bytes());
    assert!(parsed.errors().next().is_none(), "invalid fixture: {source}");
    let program = parsed.node().as_program_node().unwrap();
    let statement = program.statements().body().iter().next().unwrap();
    extract_sig(&statement)
}

fn index_for(source: &str) -> ProjectIndex {
    let db = Db::default();
    let file = SourceFile::new(&db, "inline/sorbet_contract_parser.rb".into(), source.to_owned());
    ProjectFiles::new(&db, vec![file]);
    itaruby_semantic::project_index(&db).clone()
}

#[test]
fn named_nested_types_preserve_prism_argument_boundaries() {
    let sig = signature(
        "sig(:final) do\n override.params(items: T::Hash[Symbol, T.nilable(T::Array[::ParserItem])], callback: T.proc.params(x: Integer, y: String).returns(Symbol)).returns(T.any(String, Integer)).checked(:never)\nend",
    )
    .unwrap();
    assert_eq!(
        sig.params,
        vec![
            ("items".into(), "T::Hash[Symbol, T.nilable(T::Array[::ParserItem])]".into()),
            ("callback".into(), "T.proc.params(x: Integer, y: String).returns(Symbol)".into()),
        ]
    );
    assert_eq!(sig.ret.as_deref(), Some("T.any(String, Integer)"));
    assert!(!sig.void);
    assert_eq!(sorbet_ret_ty(&sig.params[1].1), Ty::Unknown);
}

#[test]
fn void_does_not_claim_nil_return() {
    let sig = signature("sig { abstract.params(value: Integer).void }").unwrap();
    assert!(sig.void);
    assert_eq!(sig.ret, None);
    assert_eq!(sig.params, vec![("value".into(), "Integer".into())]);
}

#[test]
fn unrelated_calls_and_nested_blocks_never_supply_a_contract() {
    for source in [
        "other.sig { returns(Integer) }",
        "sig(:custom) { returns(Integer) }",
        "sig { other.returns(Integer) }",
        "sig { wrapper { returns(Integer) } }",
        "sig { returns(Integer) { returns(String) } }",
        "sig { |builder| builder.returns(Integer) }",
        "sig { returns(Integer); returns(String) }",
        "sig { override&.returns(Integer) }",
        "sig { custom.returns(Integer) }",
        "sig { type_parameters(:U).params(value: T.type_parameter(:U)).returns(Integer) }",
        "sig { bind(ParserItem).returns(Integer) }",
    ] {
        assert_eq!(signature(source), None, "unexpected contract for {source}");
    }
    assert_eq!(signature("sig { override.returns(Integer) }").unwrap().ret.as_deref(), Some("Integer"));
}

#[test]
fn duplicate_or_dynamic_clauses_fail_closed() {
    for source in [
        "sig { returns(Integer).returns(String) }",
        "sig { returns(Integer).void }",
        "sig { params(x: Integer).params(y: String).void }",
        "sig { params(x: Integer, x: String).void }",
        "sig { params(**types).returns(Integer) }",
        "sig { params(name => Integer).returns(Integer) }",
        "sig { params(\"name\" => Integer).returns(Integer) }",
        "sig { params(:\"name?\" => Integer).returns(Integer) }",
        "sig { returns(*types) }",
        "sig { returns(Integer, String) }",
        "sig { void(Integer) }",
        "sig { override.override.returns(Integer) }",
        "sig { returns(Integer).checked(mode) }",
    ] {
        assert_eq!(signature(source), None, "unexpected contract for {source}");
    }
}

#[test]
fn lexical_and_absolute_names_remain_distinct_inside_compounds() {
    let index = index_for(
        "class ParserItem; end\nmodule ParserScope\n class ParserItem; end\nend\n",
    );
    let nesting = vec!["ParserScope".into()];
    let local = Ty::Instance(index.by_path["ParserScope::ParserItem"]);
    let root = Ty::Instance(index.by_path["ParserItem"]);
    assert_eq!(resolve_sig_ty("ParserItem", &index, &nesting), local);
    assert_eq!(resolve_sig_ty("::ParserItem", &index, &nesting), root);
    assert_eq!(
        resolve_sig_ty("T::Hash[ParserItem, T.nilable(::ParserItem)]", &index, &nesting),
        Ty::Hash(Box::new(local), Box::new(Ty::union(root, Ty::Nil)))
    );
}

#[test]
fn lexical_shadowing_does_not_fall_back_to_global_scalar_or_nominal() {
    let index = index_for(
        "class ParserItem; end\nmodule ParserScope\n class String; end\n ParserItem = opaque\n Integer = opaque\nend\n",
    );
    let nesting = vec!["ParserScope".into()];
    assert_eq!(
        resolve_sig_ty("String", &index, &nesting),
        Ty::Instance(index.by_path["ParserScope::String"])
    );
    assert_eq!(resolve_sig_ty("::String", &index, &nesting), Ty::Str);
    assert_eq!(resolve_sig_ty("Integer", &index, &nesting), Ty::Unknown);
    assert_eq!(resolve_sig_ty("::Integer", &index, &nesting), Ty::Int);
    assert_eq!(resolve_sig_ty("ParserItem", &index, &nesting), Ty::Unknown);
    assert_eq!(
        resolve_sig_ty("::ParserItem", &index, &nesting),
        Ty::Instance(index.by_path["ParserItem"])
    );
}

#[test]
fn qualified_type_cannot_escape_a_dynamically_shadowed_prefix() {
    let index = index_for(
        "module Payload; class Item; end; end\nmodule Scope\n Payload = Class.new { const_set(:Item, Class.new) }\nend\n",
    );
    let nesting = vec!["Scope".into()];
    let root = Ty::Instance(index.by_path["Payload::Item"]);
    assert_eq!(resolve_sig_ty("Payload::Item", &index, &nesting), Ty::Unknown);
    assert_eq!(resolve_sig_ty("::Payload::Item", &index, &nesting), root);
    assert_eq!(
        resolve_sig_ty("T::Array[Payload::Item]", &index, &nesting),
        Ty::Array(Box::new(Ty::Unknown))
    );
    assert_eq!(
        resolve_sig_ty("T::Array[::Payload::Item]", &index, &nesting),
        Ty::Array(Box::new(root))
    );
}

#[test]
fn nearer_namespace_precedes_an_outer_dynamic_prefix_binding() {
    let index = index_for(
        "module Outer\n Payload = opaque\n module Inner\n module Payload; class Item; end; end\n end\nend\n",
    );
    let nesting = vec!["Outer".into(), "Outer::Inner".into()];
    assert_eq!(
        resolve_sig_ty("Payload::Item", &index, &nesting),
        Ty::Instance(index.by_path["Outer::Inner::Payload::Item"])
    );
}

#[test]
fn unknown_members_are_retained_without_erasing_collection_categories() {
    for expression in [
        "T.nilable(T.untyped)",
        "T.any(Integer, T.untyped)",
    ] {
        assert_eq!(sorbet_ret_ty(expression), Ty::Unknown, "{expression}");
    }
    let unknown_array = Ty::Array(Box::new(Ty::Unknown));
    assert_eq!(sorbet_ret_ty("T::Array[T.untyped]"), unknown_array);
    assert_eq!(
        sorbet_ret_ty("T::Hash[String, T.untyped]"),
        Ty::Hash(Box::new(Ty::Str), Box::new(Ty::Unknown))
    );
    assert_eq!(
        sorbet_ret_ty("T::Hash[T.untyped, Integer]"),
        Ty::Hash(Box::new(Ty::Unknown), Box::new(Ty::Int))
    );
    assert_eq!(
        sorbet_ret_ty("T.any(T::Array[T.untyped], Integer)"),
        Ty::union(unknown_array.clone(), Ty::Int)
    );
    assert_eq!(
        sorbet_ret_ty("T::Array[T.nilable(T.type_parameter(:U))]"),
        unknown_array
    );
    assert_eq!(
        sorbet_ret_ty("T::Array[T.nilable(Integer)]"),
        Ty::Array(Box::new(Ty::union(Ty::Int, Ty::Nil)))
    );
}

#[test]
fn unsupported_shapes_and_procs_retain_the_known_collection_category() {
    for expression in [
        "T::Array[{foo: Integer}]",
        "T::Array[T.proc.params(value: {foo: Integer}).void]",
    ] {
        assert_eq!(
            sorbet_ret_ty(expression),
            Ty::Array(Box::new(Ty::Unknown)),
            "{expression}"
        );
    }
    assert_eq!(
        sorbet_ret_ty("T::Hash[Symbol, {foo: Integer}]"),
        Ty::Hash(Box::new(Ty::Sym), Box::new(Ty::Unknown))
    );
}

#[test]
fn malformed_type_boundaries_never_drop_an_empty_or_unknown_member() {
    for expression in [
        "T.any(Integer,)",
        "T.any(,Integer)",
        "T.any(Integer)",
        "T::Hash[String,,Integer]",
        "T::Hash[String, Integer,]",
        "T.nilable(Integer).returns(String)",
        "T::Array[Integer][String]",
        "T::Hash[String, T::Array[Integer)]",
        "T::Array[Integer, String]",
    ] {
        assert_eq!(sorbet_ret_ty(expression), Ty::Unknown, "{expression}");
    }
}
