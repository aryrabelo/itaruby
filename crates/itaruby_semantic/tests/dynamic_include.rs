//! Dynamic `include`/`extend`/`prepend` (bead ita-d0j): a mixin call whose
//! receiver isn't implicit/`self` opens the class regardless of the
//! argument shape — `T.unsafe(self).include Rails.application.routes.
//! url_helpers`-style code was silently ignored by the `DefWalker`'s
//! generic "any call with a receiver is not a definition" bailout, so the
//! class stayed closed and a real method (arriving via the dynamic mixin)
//! was reported as unknown (E0101, a false positive violating invariant
//! #1). Asserts directly against `ProjectIndex::lookup_method`/
//! `lookup_singleton` (not `check_file`) so the fix is pinned at the layer
//! that actually decides `Found`/`NotFound`/`Inconclusive`, independent of
//! how (or whether) a given call shape surfaces a diagnostic at the CLI
//! layer.

use itaruby_semantic::index::MethodLookup;

fn index_for(name: &str) -> itaruby_semantic::ProjectIndex {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/dynamic_include");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap();
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, path.clone().into(), text);
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    itaruby_semantic::project_index(&db).clone()
}

fn class_id(index: &itaruby_semantic::ProjectIndex, path: &str) -> itaruby_semantic::types::ClassId {
    *index
        .by_path
        .get(path)
        .unwrap_or_else(|| panic!("class `{path}` not indexed"))
}

/// (a) `include <dynamic expr>` with an implicit receiver: the argument
/// isn't a literal constant path, so the class must open.
#[test]
fn implicit_receiver_dynamic_include_is_inconclusive() {
    let index = index_for("implicit_dynamic.rb");
    let id = class_id(&index, "ImplicitDynamicMixin");
    assert!(matches!(
        index.lookup_method(id, "dynamic_only_method"),
        MethodLookup::Inconclusive
    ));
}

/// (b) `Wrapper.unsafe(self).include <dynamic expr>`: an arbitrary receiver
/// — the exact shape of the confirmed false positive. Must open even
/// though nothing about the receiver expression is itself a constant.
#[test]
fn arbitrary_receiver_dynamic_include_is_inconclusive() {
    let index = index_for("unsafe_receiver_dynamic.rb");
    let id = class_id(&index, "UnsafeReceiverMixin");
    assert!(matches!(
        index.lookup_method(id, "dynamic_only_method"),
        MethodLookup::Inconclusive
    ));
}

/// (c) `Wrapper.unsafe(self).extend <dynamic expr>`: same arbitrary
/// receiver, but for singleton method resolution.
#[test]
fn arbitrary_receiver_dynamic_extend_is_inconclusive() {
    let index = index_for("extend_dynamic.rb");
    let id = class_id(&index, "DynamicExtendTarget");
    assert!(matches!(
        index.lookup_singleton(id, "dynamic_helper_method"),
        MethodLookup::Inconclusive
    ));
}

/// (d) `Wrapper.unsafe(self).prepend <dynamic expr>`: same arbitrary
/// receiver, for instance method resolution via `prepend`.
#[test]
fn arbitrary_receiver_dynamic_prepend_is_inconclusive() {
    let index = index_for("prepend_dynamic.rb");
    let id = class_id(&index, "DynamicPrependTarget");
    assert!(matches!(
        index.lookup_method(id, "dynamic_only_method"),
        MethodLookup::Inconclusive
    ));
}

/// (e) guard rail: `include <LiteralConstant>` with an implicit receiver
/// must keep closing ancestry exactly as before — a defined method from
/// the included module resolves `Found`, and a genuinely unknown method
/// resolves `NotFound` (never silenced by this bead's generalization).
#[test]
fn literal_include_with_implicit_receiver_still_closes_ancestry() {
    let index = index_for("literal_include_closes.rb");
    let id = class_id(&index, "LiteralIncludeTarget");
    assert!(matches!(index.lookup_method(id, "hello"), MethodLookup::Found(..)));
    assert!(matches!(
        index.lookup_method(id, "nonexistent_method"),
        MethodLookup::NotFound
    ));
}
