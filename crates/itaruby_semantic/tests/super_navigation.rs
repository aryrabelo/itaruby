//! `super` navigation (bead ita-53y): `SuperNode`/`ForwardingSuperNode`
//! previously only ran type inference on their arguments and never called
//! any navigation hook, so `ita definition` on a `super` call site always
//! answered `unknown` — legitimate silence under invariant #1, but a real
//! coverage gap. Fix: continue the linearization from the ancestor
//! strictly after the one that physically defines the currently executing
//! method (`ProjectIndex::super_lookup`), never from the start and never a
//! guess. Asserts directly against `super_lookup` (not `definition_at`),
//! same layer `dynamic_include.rs` pins its fix at.

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

/// `super` inside a method defined directly on a class continues into that
/// class's own superclass chain — `SuperChild#greet`'s `super` must reach
/// `SuperBase#greet`.
#[test]
fn super_in_class_method_resolves_to_superclass() {
    let index = index_for("super_class.rb");
    let child = class_id(&index, "SuperChild");
    match index.super_lookup(child, false, "greet") {
        MethodLookup::Found(m, owner) => {
            assert_eq!(owner, class_id(&index, "SuperBase"));
            assert!(!m.arity_unknown);
        }
        other => panic!("expected Found(SuperBase#greet), got {other:?}"),
    }
}

/// `super` inside a method defined on a *prepended module* has no chain of
/// its own: it must continue into the one class that actually prepends the
/// module, landing on that class's own method right after the module's
/// slot in the MRO — `SuperLoudAnnounce#greet`'s `super` must reach
/// `SuperAnnouncer#greet`, never restart from the top of the chain.
#[test]
fn super_in_prepended_module_method_resolves_to_consuming_class() {
    let index = index_for("super_prepend.rb");
    let module = class_id(&index, "SuperLoudAnnounce");
    match index.super_lookup(module, false, "greet") {
        MethodLookup::Found(m, owner) => {
            assert_eq!(owner, class_id(&index, "SuperAnnouncer"));
            assert!(!m.arity_unknown);
        }
        other => panic!("expected Found(SuperAnnouncer#greet), got {other:?}"),
    }
}

/// `super` where the superclass is unresolvable (`ExternalSuperBase` isn't
/// in the index) must stay silent — never guess at the next ancestor when
/// the chain is provably incomplete.
#[test]
fn super_with_incomplete_ancestry_is_inconclusive() {
    let index = index_for("super_incomplete.rb");
    let base = class_id(&index, "SuperIncompleteBase");
    assert!(matches!(
        index.super_lookup(base, false, "greet"),
        MethodLookup::Inconclusive
    ));
}
