//! Singleton-track: the mocking gems' class-object surface
//! (`X.any_instance`). The residue probe at de28b40 measured ALL 160 of
//! discourse's explicit-receiver residue sites as
//! `SomeClass.any_instance` — while neither rspec-mocks nor mocha is
//! declared anywhere in the curated declarations, so the softening
//! rides the project's own `Gemfile.lock` instead, keyed on the METHOD
//! NAME (AGENTS.md: a receiver-blind softening must key on the name —
//! measured twice in the dynamic-mixin history).
//!
//! Fixtures live in `testdata/mock_singleton_surface/`, each directory
//! a single-file project with its own `Gemfile.lock`, discovered for
//! real by `wire_declaration_sources` exactly the way
//! `ita check`/`ita server` discover one.
//!
//! MRI ground truth (verified on the machine that measured the corpus):
//! with `rspec/mocks` required, `Zed.any_instance` returns
//! `RSpec::Mocks::AnyInstance::Proxy`; without the gem it raises
//! `NoMethodError`.

use std::path::PathBuf;

use itaruby_semantic::index::MethodLookup;
use itaruby_semantic::{project_index, Db, ProjectFiles, SourceFile};

/// Builds the real index for one fixture project — lock discovery and
/// all — and returns it, so the assertions below read the exact
/// structure `soften_not_found` consults.
fn index_in(dir: &str) -> itaruby_semantic::ProjectIndex {
    let root = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/mock_singleton_surface"
    ))
    .join(dir);
    let path = root.join("any_instance_call.rb");
    let text = std::fs::read_to_string(&path).unwrap();
    let mut db = Db::default();
    itaruby_semantic::wire_declaration_sources(&mut db, &[root]);
    let file = SourceFile::new(&db, path, text.clone());
    ProjectFiles::new(&db, vec![file]);
    project_index(&db).clone()
}

/// With rspec-mocks/mocha in the lock, `any_instance` softens to
/// `Inconclusive` — the population really supplies the name at runtime.
#[test]
fn any_instance_softens_when_a_mock_gem_is_locked() {
    let index = index_in("with_mock_gem");
    let id = *index.by_path.get("Invoice").expect("fixture class indexed");
    assert!(
        matches!(
            index.lookup_singleton_rbi(id, "any_instance", None),
            MethodLookup::Inconclusive
        ),
        "any_instance must soften while a mocking gem is locked"
    );
}

/// NAME-KEYED, never blanket: the same locked project keeps a
/// conclusive `NotFound` for a name the gem does not install — a typo
/// on `any_instance` must never ride the softening.
#[test]
fn the_softening_keys_on_the_name_not_the_track() {
    let index = index_in("with_mock_gem");
    let id = *index.by_path.get("Invoice").expect("fixture class indexed");
    assert!(
        matches!(
            index.lookup_singleton_rbi(id, "any_instence", None),
            MethodLookup::NotFound
        ),
        "a near-miss name must stay conclusively NotFound"
    );
}

/// POPULATION-GATED: a lock that names no mocking gem keeps
/// `any_instance` conclusively `NotFound` — at runtime that call is a
/// certain `NoMethodError`, and this gate is what keeps the checker's
/// future E0101 on the singleton track able to accuse it.
#[test]
fn any_instance_stays_not_found_without_a_mock_gem() {
    let index = index_in("without_mock_gem");
    let id = *index.by_path.get("Invoice").expect("fixture class indexed");
    assert!(
        matches!(
            index.lookup_singleton_rbi(id, "any_instance", None),
            MethodLookup::NotFound
        ),
        "without a mocking gem the population does not exist"
    );
    assert!(
        index.mock_singleton_methods.is_empty(),
        "no mocking gem in the lock, no population"
    );
}
