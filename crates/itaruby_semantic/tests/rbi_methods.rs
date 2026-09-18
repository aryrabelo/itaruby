//! `index.rs::rbi_method_lookup` (beads ita-xze, ita-uh1): does an EXTERNAL
//! ancestor of a project class — a superclass/mixin name the project's own
//! index never resolved, or a `declarations/gems.rbi` force-open entry
//! (`OpenReason::DeclaredExternal`) — declare a method in the client's
//! Tapioca RBIs? Aditive-only: every test here proves either a real `Some`
//! (an RBI genuinely declares the method somewhere in the external chain —
//! the wrapped `Ty` is `Ty::Unknown` for every fixture here, since none of
//! these toy RBIs carry a sorbet sig; the sig-mapping contract itself lives
//! in `rbi_ret_ty.rs`) or a `None` that changes nothing observable.
//! Fixtures are toy `.rbi` files written to a per-test tempdir
//! (`std::env::temp_dir()`, mirroring `crates/itaruby/tests/hover_cli.rs`'s
//! pattern), never `testdata/` — `ita check testdata/` scans the whole tree
//! as one project and a planted fixture there would leak diagnostics across
//! unrelated tests.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("itaruby-rbi-methods-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Writes one `.rbi` file under `<dir>/sorbet/rbi/gems/` and returns the
/// `constant -> file` map `discover_rbi_files`/`build_rbi_index` produce
/// for it — the exact `rbi_map` shape `rbi_method_lookup` takes.
fn write_rbi(dir: &Path, content: &str) -> HashMap<String, Vec<PathBuf>> {
    let rbi_dir = dir.join("sorbet/rbi/gems");
    std::fs::create_dir_all(&rbi_dir).unwrap();
    std::fs::write(rbi_dir.join("gem.rbi"), content).unwrap();
    let files = itaruby_semantic::rbi::discover_rbi_files(&dir.join("sorbet/rbi"));
    itaruby_semantic::rbi::build_rbi_index(&files).constants
}

/// One project file's `ProjectIndex`, independent of the checker plumbing
/// — same "direct query" pattern as `rbi_ancestry.rs`'s
/// `closure_collects_mixin_constants_through_superclass_chain`. `Db` is
/// local and dropped at the end of this call; `ProjectIndex` is an owned
/// value (no `'db` lifetime), so nothing borrows past the return.
fn project_index_for(dir: &Path, text: &str) -> itaruby_semantic::index::ProjectIndex {
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, dir.join("project.rb"), text.to_string());
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    itaruby_semantic::project_index(&db).clone()
}

fn class_id(
    index: &itaruby_semantic::index::ProjectIndex,
    path: &str,
) -> itaruby_semantic::types::ClassId {
    *index
        .by_path
        .get(path)
        .unwrap_or_else(|| panic!("{path} must be interned in the project index"))
}

/// Population 1 (measured 45.7% of the `ancestry open` bucket): a
/// superclass name the project's own index never resolved at all. The RBI
/// declares the method directly on that unresolved name — the exact shape
/// `unresolved_ancestors` was already collecting for constant lookup
/// (`external_lookup_starts`), reused here as a method-lookup start.
#[test]
fn unresolved_superclass_method_resolves_through_rbi() {
    let dir = tempdir("unresolved-super");
    let map = write_rbi(
        &dir,
        "class RbiMethGemA::Base\n  def greet(x)\n  end\n\n  def self.build(x)\n  end\nend\n",
    );
    let proj = project_index_for(
        &dir,
        "class RbiMethHitsUnresolvedSuper < RbiMethGemA::Base\nend\n",
    );
    let id = class_id(&proj, "RbiMethHitsUnresolvedSuper");

    assert!(
        itaruby_semantic::index::rbi_method_lookup(&proj, id, "greet", false, &map).is_some(),
        "instance method declared on the unresolved superclass must resolve"
    );
    assert!(
        itaruby_semantic::index::rbi_method_lookup(&proj, id, "build", true, &map).is_some(),
        "singleton method declared on the unresolved superclass must resolve"
    );
}

/// Singleton and instance never cross for a plain (non-`extend`) hit: a
/// name declared only as an instance method must not answer a singleton
/// query, and vice versa — `def self.build` and `def greet` are on the
/// SAME RBI class in `unresolved_superclass_method_resolves_through_rbi`'s
/// fixture, so a naive "does the name exist anywhere on this class"
/// implementation would wrongly pass both directions.
#[test]
fn singleton_and_instance_hits_stay_on_their_own_side() {
    let dir = tempdir("no-cross");
    let map = write_rbi(
        &dir,
        "class RbiMethGemB::Base\n  def greet(x)\n  end\n\n  def self.build(x)\n  end\nend\n",
    );
    let proj = project_index_for(&dir, "class RbiMethNoCrossSub < RbiMethGemB::Base\nend\n");
    let id = class_id(&proj, "RbiMethNoCrossSub");

    assert!(
        itaruby_semantic::index::rbi_method_lookup(&proj, id, "greet", true, &map).is_none(),
        "an instance-only method must not answer a singleton query"
    );
    assert!(
        itaruby_semantic::index::rbi_method_lookup(&proj, id, "build", false, &map).is_none(),
        "a singleton-only method must not answer an instance query"
    );
}

/// Population 2 (measured 40.2% of the `ancestry open` bucket, and the one
/// `unresolved_ancestors` structurally cannot see): the project class
/// inherits from `ActiveRecord::Base`, which itaruby's own curated
/// `declarations/gems.rbi` force-opens (`OpenReason::DeclaredExternal`) —
/// the name RESOLVES in the project index, it just carries zero methods by
/// that file's contract. The toy RBI here declares the exact same
/// fully-qualified path, so the resolved-but-empty ancestor's own `path`
/// is what must drive the RBI walk.
#[test]
fn declared_external_ancestor_method_resolves_through_rbi() {
    let dir = tempdir("declared-external");
    let map = write_rbi(
        &dir,
        "class ActiveRecord::Base\n  def find_by_custom(x)\n  end\nend\n",
    );
    let proj = project_index_for(
        &dir,
        "class RbiMethHitsDeclaredExternal < ActiveRecord::Base\nend\n",
    );
    let id = class_id(&proj, "RbiMethHitsDeclaredExternal");

    assert!(
        itaruby_semantic::index::rbi_method_lookup(&proj, id, "find_by_custom", false, &map)
            .is_some(),
        "a method declared on the DeclaredExternal ancestor's own RBI must resolve"
    );
}

/// If population 2 (the `DeclaredExternal` walk) is ever removed from
/// `external_ancestor_starts`, `unresolved_superclass_method_resolves_through_rbi`
/// still passes (population 1 is untouched) but this one goes from `Some`
/// to `None` — a curated `gems.rbi` name resolves cleanly in the project
/// index, so `unresolved_ancestors` never names it. This test is the one
/// that actually falls over without population 2.
///
/// Uses a DIFFERENT curated name than the test above on purpose:
/// `rbi_method_closure`'s memo is keyed on the start name alone, so two
/// tests in this binary sharing one external ancestor would serve each
/// other's cached method set and fail by run order, not by behavior.
#[test]
fn declared_external_ancestor_is_not_covered_by_unresolved_names_alone() {
    let dir = tempdir("declared-external-proof");
    let map = write_rbi(
        &dir,
        "class ActiveStorage::Blob\n  def only_on_declared_external(x)\n  end\nend\n",
    );
    let proj = project_index_for(
        &dir,
        "class RbiMethDeclaredExternalProof < ActiveStorage::Blob\nend\n",
    );
    let id = class_id(&proj, "RbiMethDeclaredExternalProof");
    // `ActiveRecord::Base` really did resolve — this is exactly what makes
    // population 2 necessary: `unresolved_ancestors` would find nothing.
    assert!(
        proj.ancestors(id).1,
        "ActiveStorage::Blob must resolve cleanly (it is curated, not unresolved)"
    );
    assert!(itaruby_semantic::index::rbi_method_lookup(
        &proj,
        id,
        "only_on_declared_external",
        false,
        &map
    )
    .is_some());
}

/// Hit two hops deep in the RBI world: the project's unresolved
/// superclass (`RbiMethChain::Base`) itself has a superclass
/// (`RbiMethChain::Root`, superclass edge) and an `include`
/// (`RbiMethChain::Extras`, include edge) declared only inside the RBI —
/// both must be walked, not just the immediate start.
#[test]
fn method_two_hops_deep_through_superclass_and_include_resolves() {
    let dir = tempdir("transitive");
    let map = write_rbi(
        &dir,
        "class RbiMethChain::Root\n  def root_method(x)\n  end\nend\n\
         class RbiMethChain::Base < RbiMethChain::Root\n  include RbiMethChain::Extras\nend\n\
         module RbiMethChain::Extras\n  def extra_method(x)\n  end\nend\n",
    );
    let proj = project_index_for(
        &dir,
        "class RbiMethTransitiveSub < RbiMethChain::Base\nend\n",
    );
    let id = class_id(&proj, "RbiMethTransitiveSub");

    assert!(
        itaruby_semantic::index::rbi_method_lookup(&proj, id, "extra_method", false, &map)
            .is_some(),
        "method reachable via one include hop must resolve"
    );
    assert!(
        itaruby_semantic::index::rbi_method_lookup(&proj, id, "root_method", false, &map)
            .is_some(),
        "method reachable via one superclass hop must resolve"
    );
}

/// The `extend` crossing (lead review, bead ita-xze, 2026-08-21): a method
/// that exists ONLY as an INSTANCE method of a module reached via
/// `extend` must answer `Some` for a SINGLETON query and `None` for an
/// INSTANCE query — this is the real shape of `Model.where`, where
/// `where` is an instance method of `ActiveRecord::Querying` and
/// `ActiveRecord::Base` does `extend ::ActiveRecord::Querying`. Without
/// the extend-to-singleton track switch, this method is invisible to
/// `rbi_method_lookup` no matter which side is queried.
#[test]
fn method_reached_only_through_extend_answers_singleton_not_instance() {
    let dir = tempdir("extend-crossing");
    let map = write_rbi(
        &dir,
        "class RbiMethExtendGem::Base\n  extend RbiMethExtendGem::Querying\nend\n\
         module RbiMethExtendGem::Querying\n  def where(x)\n  end\nend\n",
    );
    let proj = project_index_for(
        &dir,
        "class RbiMethExtendSub < RbiMethExtendGem::Base\nend\n",
    );
    let id = class_id(&proj, "RbiMethExtendSub");

    assert!(
        itaruby_semantic::index::rbi_method_lookup(&proj, id, "where", true, &map).is_some(),
        "an extended module's instance method must answer a singleton query"
    );
    assert!(
        itaruby_semantic::index::rbi_method_lookup(&proj, id, "where", false, &map).is_none(),
        "an extended module's instance method must NOT answer an instance query"
    );
}

/// Miss: the RBI genuinely exists and is wired, but never declares the
/// queried name anywhere in the external ancestor's chain.
#[test]
fn name_the_rbi_never_declares_stays_a_miss() {
    let dir = tempdir("miss-undeclared");
    let map = write_rbi(
        &dir,
        "class RbiMethGemC::Base\n  def real_method(x)\n  end\nend\n",
    );
    let proj = project_index_for(&dir, "class RbiMethMissSub < RbiMethGemC::Base\nend\n");
    let id = class_id(&proj, "RbiMethMissSub");

    assert!(itaruby_semantic::index::rbi_method_lookup(
        &proj,
        id,
        "not_a_real_method",
        false,
        &map
    )
    .is_none());
}

/// Miss: the project class's own ancestry is fully closed (no
/// superclass, no include/prepend at all) — there is no external ancestor
/// to consult in the first place. The `rbi_map` passed in is a fully
/// populated, real map (it even declares a method of the SAME name being
/// queried, on an unrelated external namespace) to prove this isn't a
/// weak "empty map" miss: a closed project class must never pick up an
/// RBI method just because the name happens to exist somewhere in the
/// wired RBI world.
#[test]
fn closed_project_ancestry_never_leaks_an_unrelated_rbi_method() {
    let dir = tempdir("closed-chain");
    let map = write_rbi(
        &dir,
        "class RbiMethGemD::Unrelated\n  def shared_name(x)\n  end\nend\n",
    );
    let proj = project_index_for(&dir, "class RbiMethClosedChain\nend\n");
    let id = class_id(&proj, "RbiMethClosedChain");

    assert!(
        itaruby_semantic::index::rbi_method_lookup(&proj, id, "shared_name", false, &map)
            .is_none(),
        "a closed ancestry must never consult the RBI world at all"
    );
}

/// Two RBI classes whose superclass edges point at each other: the BFS
/// must still terminate (via the cap/visited guard) and still find a
/// method declared before the cycle closes.
#[test]
fn cyclic_rbi_declarations_terminate_and_still_resolve() {
    let dir = tempdir("cycle");
    let map = write_rbi(
        &dir,
        "class RbiMethCycleA < RbiMethCycleB\n  def a_method(x)\n  end\nend\n\
         class RbiMethCycleB < RbiMethCycleA\n  def b_method(x)\n  end\nend\n",
    );
    let proj = project_index_for(&dir, "class RbiMethCycleSub < RbiMethCycleA\nend\n");
    let id = class_id(&proj, "RbiMethCycleSub");

    assert!(
        itaruby_semantic::index::rbi_method_lookup(&proj, id, "a_method", false, &map).is_some()
    );
    assert!(
        itaruby_semantic::index::rbi_method_lookup(&proj, id, "b_method", false, &map).is_some()
    );
    assert!(itaruby_semantic::index::rbi_method_lookup(
        &proj,
        id,
        "never_declared",
        false,
        &map
    )
    .is_none());
}
