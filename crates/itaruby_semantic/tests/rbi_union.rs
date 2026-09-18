//! Bead ita-k9j.3: `rbi.rs::build_rbi_index` must UNION every file that
//! declares a constant, never pick one arbitrarily. Before this fix,
//! `RbiIndex::constants` was `HashMap<String, PathBuf>` built with
//! `.entry(..).or_insert_with(..)` — first occurrence wins, and
//! `discover_rbi_files`'s `read_dir` walk carries no ordering guarantee.
//! Measured on a real Tapioca corpus: three files declare
//! `class ActiveRecord::Base` — the real `activerecord@*.rbi` (123 direct
//! edges) plus two reopening stubs, `activestorage@*.rbi` (8 edges) and
//! `flipper@*.rbi` (5 edges). Whichever stub won the filesystem race, the
//! RBI method closure BFS (`index.rs::rbi_method_closure`) silently
//! started from 5 or 8 edges instead of 123 — `rbi_escalate`'s RBI step
//! (bead ita-xze) stopped escalating for most methods, invisibly, because
//! the degraded result was `Inconclusive`, safe under invariant #1.
//!
//! Fixtures live under `testdata/rbi_union/sorbet/rbi/gems/` (`full.rbi`,
//! a real declaration reaching a distinctive method two `include` hops
//! deep; `stub.rbi`, a thin same-class reopening that never reaches it) —
//! globally unique `RbiUnion*` names per AGENTS.md/bead ita-u1t (`ita
//! check testdata/` scans the whole tree as ONE project). This file never
//! relies on `discover_rbi_files`'s own (now sorted) order: every test
//! builds its `files: Vec<PathBuf>` by hand, in the EXACT order under
//! test, so a regression to first-wins is caught regardless of which
//! order this machine's `read_dir` would have produced.
//!
//! Every test below queries a DISTINCT external ancestor name
//! (`RbiUnionOrderA`/`B`/`C`/`D`) even though several share the same
//! full+stub shape: `rbi_method_closure`'s BFS result is memoized
//! process-wide, keyed on the start name alone (see
//! `tests/rbi_methods.rs`'s header comment) — reusing a name across two
//! assertions that expect different discovery orders would silently
//! replay the first order's cached result and prove nothing about the
//! second.
//!
//! MUTANTS THIS FILE MUST CATCH:
//!
//!   1. Revert `rbi.rs::build_rbi_index` to first-wins
//!      (`.entry(..).or_insert_with(|| path.clone())` instead of
//!      `.entry(..).or_default().push(path.clone())`) ->
//!      `full_declaration_resolves_when_full_file_is_discovered_first`
//!      and `full_declaration_resolves_when_stub_file_is_discovered_first`
//!      cannot BOTH pass: whichever file `discover_rbi_files` (real,
//!      order-dependent `read_dir`, faked here by hand) enumerates first
//!      wins the single slot, so exactly one of the two orders loses
//!      `order_a_full_only_method`/`order_b_full_only_method`.
//!   2. Union the instance and singleton method maps together in
//!      `harvest_frag_methods`/`compute_rbi_method_closure` (e.g. always
//!      writing to `instance` regardless of `on_singleton_track`) ->
//!      `union_keeps_each_files_methods_on_its_own_track` fails: the
//!      full declaration's instance method would answer a singleton
//!      query, or the stub's `extend`-reached singleton method would
//!      answer an instance query.

use std::collections::HashMap;
use std::path::PathBuf;

fn fixture_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/rbi_union"))
        .join("sorbet/rbi/gems")
}

fn full_rbi() -> PathBuf {
    fixture_dir().join("full.rbi")
}

fn stub_rbi() -> PathBuf {
    fixture_dir().join("stub.rbi")
}

/// Builds the `constant -> every declaring file` map from an EXPLICIT
/// file order, bypassing `discover_rbi_files`'s own (now sorted, bead
/// ita-k9j.3) walk entirely — the whole point of this file is to drive
/// both possible discovery orders on demand, not to hope the real
/// filesystem happens to produce one or the other.
fn build_map_in_order(files: &[PathBuf]) -> HashMap<String, Vec<PathBuf>> {
    itaruby_semantic::rbi::build_rbi_index(files).constants
}

/// One project file's `ProjectIndex`, independent of the checker plumbing
/// — same "direct query" pattern as `tests/rbi_methods.rs`'s
/// `project_index_for`. `Db` is local and dropped at the end of this
/// call; `ProjectIndex` is an owned value, so nothing borrows past the
/// return.
fn project_index_for(text: &str) -> itaruby_semantic::index::ProjectIndex {
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(
        &db,
        fixture_dir().join("inline.rb"),
        text.to_string(),
    );
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

/// Order 1: `full.rbi` discovered before `stub.rbi`. The method declared
/// only two `include` hops deep inside `full.rbi` must resolve.
#[test]
fn full_declaration_resolves_when_full_file_is_discovered_first() {
    let map = build_map_in_order(&[full_rbi(), stub_rbi()]);
    let proj = project_index_for("class RbiUnionConsumerA < RbiUnionOrderA::Base\nend\n");
    let id = class_id(&proj, "RbiUnionConsumerA");

    assert!(
        itaruby_semantic::index::rbi_method_lookup(
            &proj,
            id,
            "order_a_full_only_method",
            false,
            &map
        )
        .is_some(),
        "the full declaration's deep method must resolve even when it is discovered first \
         (proves the ordinary case still works, not just the reversed one)"
    );
}

/// Order 2: `stub.rbi` discovered before `full.rbi` — the reversed
/// order, using a DISTINCT constant name so the process-wide BFS memo
/// cannot mask a real order dependency. Before this bead, this is
/// exactly the case that silently dropped `full.rbi`'s edges: a
/// first-wins map would record only `stub.rbi` for `RbiUnionOrderB::Base`
/// here, and the BFS would never even open `full.rbi`.
#[test]
fn full_declaration_resolves_when_stub_file_is_discovered_first() {
    let map = build_map_in_order(&[stub_rbi(), full_rbi()]);
    let proj = project_index_for("class RbiUnionConsumerB < RbiUnionOrderB::Base\nend\n");
    let id = class_id(&proj, "RbiUnionConsumerB");

    assert!(
        itaruby_semantic::index::rbi_method_lookup(
            &proj,
            id,
            "order_b_full_only_method",
            false,
            &map
        )
        .is_some(),
        "the full declaration's deep method must resolve even when the stub reopening is \
         discovered first — this is the exact shape the bead's real corpus measurement hit"
    );
}

/// A method neither `full.rbi` nor `stub.rbi` declares anywhere in
/// `RbiUnionOrderC::Base`'s union closure must stay a miss at every
/// layer: the raw lookup returns `None` (both instance and singleton
/// sides), and — critically — the full checker never manufactures a
/// diagnostic from that miss. The escalation channel is additive-only
/// (see `index.rs::rbi_method_lookup`'s doc comment): a miss must leave
/// the call site exactly as `Inconclusive`/silent as it was before this
/// bead existed, never promote it to `NotFound`/E0101.
#[test]
fn undeclared_method_never_resolves_and_never_manufactures_a_diagnostic() {
    let map = build_map_in_order(&[full_rbi(), stub_rbi()]);
    let proj = project_index_for("class RbiUnionConsumerC < RbiUnionOrderC::Base\nend\n");
    let id = class_id(&proj, "RbiUnionConsumerC");

    assert!(
        itaruby_semantic::index::rbi_method_lookup(
            &proj,
            id,
            "order_c_never_declared_anywhere",
            false,
            &map
        )
        .is_none(),
        "a method neither file declares must stay a miss on the instance side"
    );
    assert!(
        itaruby_semantic::index::rbi_method_lookup(
            &proj,
            id,
            "order_c_never_declared_anywhere",
            true,
            &map
        )
        .is_none(),
        "a method neither file declares must stay a miss on the singleton side too"
    );

    // End-to-end: the same miss, through the real checker, wired exactly
    // like a client project (`RbiProject` singleton), proving the miss
    // never turns into a diagnostic.
    let db = itaruby_semantic::Db::default();
    itaruby_semantic::RbiProject::new(&db, map);
    let file = itaruby_semantic::SourceFile::new(
        &db,
        fixture_dir().join("consumer_c.rb"),
        "\
class RbiUnionConsumerC < RbiUnionOrderC::Base
  def caller_method
    order_c_never_declared_anywhere
  end
end
"
        .to_string(),
    );
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    let diags = itaruby_semantic::check_file(&db, file);
    assert!(
        diags.is_empty(),
        "a genuinely undeclared RBI method must never manufacture a diagnostic \
         (additive-only escalation, invariant #1): {diags:?}"
    );
}

/// Track separation under a union: `full.rbi`'s `order_d_full_only_method`
/// reaches `RbiUnionOrderD::Base` through a plain `include` chain (stays
/// on the INSTANCE track); `stub.rbi`'s `order_d_stub_singleton_only_method`
/// reaches it through an `extend` edge (switches to the SINGLETON track,
/// rule 2). Every assertion below must hold simultaneously for the union
/// to be proven correct: each method answers its OWN track and never the
/// other's, exactly as if each file's edges had been walked in complete
/// isolation.
#[test]
fn union_keeps_each_files_methods_on_its_own_track() {
    let map = build_map_in_order(&[full_rbi(), stub_rbi()]);
    let proj = project_index_for("class RbiUnionConsumerD < RbiUnionOrderD::Base\nend\n");
    let id = class_id(&proj, "RbiUnionConsumerD");

    assert!(
        itaruby_semantic::index::rbi_method_lookup(
            &proj,
            id,
            "order_d_full_only_method",
            false,
            &map
        )
        .is_some(),
        "full.rbi's instance-track method must resolve on an INSTANCE query"
    );
    assert!(
        itaruby_semantic::index::rbi_method_lookup(
            &proj,
            id,
            "order_d_full_only_method",
            true,
            &map
        )
        .is_none(),
        "full.rbi's instance-track method must NOT answer a SINGLETON query \
         (a union that blends the two files' tracks would leak it here)"
    );
    assert!(
        itaruby_semantic::index::rbi_method_lookup(
            &proj,
            id,
            "order_d_stub_singleton_only_method",
            true,
            &map
        )
        .is_some(),
        "stub.rbi's extend-reached method must resolve on a SINGLETON query"
    );
    assert!(
        itaruby_semantic::index::rbi_method_lookup(
            &proj,
            id,
            "order_d_stub_singleton_only_method",
            false,
            &map
        )
        .is_none(),
        "stub.rbi's extend-reached method must NOT answer an INSTANCE query"
    );
}
