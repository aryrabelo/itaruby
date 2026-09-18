//! `define_method(:x) do ... end` (bead ita-53y): the `DefWalker` used to
//! mark the enclosing class `open` for *any* class-body call carrying a
//! block, before ever dispatching on the call's name — so the dedicated
//! `define_method` handling (which indexes the method) was unreachable in
//! the common block form. Fix: `define_method` with a literal name (symbol
//! or string) indexes instead of opening; any other block-taking call, or
//! `define_method` with a dynamic name, still opens the class exactly as
//! before (that marking is what backs invariant #1 — see bead ita-d0j).

use itaruby_semantic::index::MethodLookup;

fn index_for(name: &str) -> itaruby_semantic::ProjectIndex {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata");
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

#[test]
fn define_method_block_with_symbol_literal_indexes() {
    let index = index_for("define_method_block.rb");
    let id = class_id(&index, "DefineMethodBlockSymbol");
    assert!(matches!(index.lookup_method(id, "greet"), MethodLookup::Found(..)));
}

#[test]
fn define_method_block_with_string_literal_indexes() {
    let index = index_for("define_method_block.rb");
    let id = class_id(&index, "DefineMethodBlockString");
    assert!(matches!(index.lookup_method(id, "greet"), MethodLookup::Found(..)));
}

/// A dynamic name (a variable, here) can't be known at index time: the
/// class must stay open, exactly as it did before this bead for every
/// block-taking call.
#[test]
fn define_method_block_with_dynamic_name_stays_open() {
    let index = index_for("define_method_block.rb");
    let id = class_id(&index, "DefineMethodBlockDynamic");
    assert!(matches!(
        index.lookup_method(id, "greet"),
        MethodLookup::Inconclusive
    ));
}
