//! Bead ita-4wq: an alias WRITTEN INSIDE a vendored RBI itself
//! (`Rbi4wqRubyLsp::Constant = Rbi4wqLanguageServer::Protocol::Constant` — Sorbet's
//! real re-export shape when one gem's RBI aliases another gem's
//! namespace) created no usable edge for the RBI expansion machinery.
//! The real tapioca/ruby-lsp measured shape needs the FULL chain:
//! bead ita-hzd's nesting expansion (`Rbi4wqRubyLsp::Constant::MessageType::WARNING`
//! from a bare `Constant::MessageType::WARNING` reference 3 levels deep
//! inside `Rbi4wqRubyLsp::Tapioca::SomeAddon`) -> THIS bead recognizing
//! `Rbi4wqRubyLsp::Constant` as an RBI-internal alias whose target is
//! `Rbi4wqLanguageServer::Protocol::Constant` -> beads ita-3dg/ita-y0s's
//! `rbi_qualified_const_declares` resolving `WARNING` as a qualified
//! `T.let` write under the expanded namespace. Before this bead, no
//! consumer had ever read an RBI file's own `FileDefs::const_aliases`
//! (bead ita-54k harvests it via the SAME `DefWalker` project files use,
//! but every existing alias-chase function only ever walked the
//! PROJECT's merged aliases) — so the chain died at the second hop and
//! this exact real-world reference fired a false E0104.
//!
//! Fix (`crates/itaruby_semantic/src/index.rs`): `rbi_alias_lookup` (is
//! `name` itself a qualified-write alias LHS somewhere in the RBI?),
//! `rbi_alias_leaf` (cycle-guarded, capped chase to a non-alias leaf),
//! and `rbi_alias_expand` (public entry point: does SOME prefix of a
//! qualified path name an RBI alias, and if so what's the expanded
//! candidate with the remaining segments re-appended?).
//! `crates/itaruby_semantic/src/check.rs`: `Checker::rbi_alias_edge_declares`
//! wires `rbi_alias_expand` into both `check_const_ref`'s direct branch
//! and `nesting_expanded_rbi_declares`'s per-candidate loop (bead
//! ita-hzd), retrying `rbi_declares`/`rbi_qualified_const_declares`/
//! `rbi_ancestor_declares` against the expanded path. Suppression-only,
//! same contract as every other RBI channel (invariant #1): a hit only
//! silences E0104, `infer_const` still types the reference
//! `Ty::Unknown`; an unresolvable RBI alias target degrades to exactly
//! today's (pre-bead) behavior, never worse.
//!
//! Fixtures under `testdata/rbi_alias_edge/`:
//! `sorbet/rbi/gems/rbi_4wq_vendored_gem.rbi` declares the alias edge
//! plus the real enum member; `sorbet/rbi/gems/rbi_4wq_cycle.rbi` is the
//! mandatory pathological `A = B; B = A` pair, written entirely inside
//! the RBI (a completely separate code path from
//! `ProjectIndex::CONST_ALIAS_CHAIN_CAP`'s project-side guard).
//!
//! MUTANTS THIS FILE MUST CATCH:
//!   (a) RBI alias edges disabled (e.g. `check.rs`'s
//!       `|| self.rbi_alias_edge_declares(...)` arms dropped, or
//!       `index.rs::rbi_alias_expand` mutated to always return `None`)
//!       -> `alias_edge_member_resolves_through_full_chain` fails,
//!       re-emitting the exact false E0104 this bead exists to kill.
//!   (b) the cycle guard removed from `rbi_alias_leaf` (both the
//!       `visited` set AND `RBI_ALIAS_CHAIN_CAP`) -> `cycle_never_hangs`
//!       fails by TIMING OUT rather than by a wrong assertion — asserted
//!       with an explicit bounded `recv_timeout`, not just "the whole
//!       test binary eventually finishes".
//!   (c) a suppression-only violation: the alias-edge hit wired into
//!       `infer_const`'s type instead of staying a pure
//!       `check_const_ref` suppression (e.g. `infer_const` starts typing
//!       an alias-edge-resolved reference `Ty::Class`/`Ty::Instance`
//!       instead of leaving it `Ty::Unknown`) ->
//!       `resolved_alias_edge_still_types_unknown` wrongly raises E0101
//!       on `bad_method_call`'s obviously undefined method call.
//!
//! Negative control (`undeclared_member_under_resolved_target_still_accuses`):
//! a member the RBI genuinely never declares under the alias's resolved
//! target must keep accusing — the alias-edge chase must never become a
//! blanket suppressor for everything under the target namespace.

fn dir() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/rbi_alias_edge")
}

/// Loads one fixture file plus this dir's own toy `sorbet/rbi`, mirroring
/// `tests/rbi.rs`'s `check_fixture` harness.
fn check_fixture(name: &str) -> Vec<String> {
    let dir = dir();
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap();
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, path.into(), text.clone());
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);

    let rbi_dir = std::path::Path::new(dir).join("sorbet/rbi");
    let files = itaruby_semantic::rbi::discover_rbi_files(&rbi_dir);
    assert!(!files.is_empty(), "fixture must carry at least one .rbi file");
    let index = itaruby_semantic::rbi::build_rbi_index(&files);
    itaruby_semantic::RbiProject::new(&db, index.constants);

    let li = itaruby_semantic::LineIndex::new(&text);
    itaruby_semantic::check_file(&db, file)
        .iter()
        .map(|d| {
            let (l, c) = li.line_col(&text, d.start);
            format!("{}:{}:{} {}", l + 1, c + 1, d.code, d.message)
        })
        .collect()
}

/// Mutant (a): the actual bug. `handle`'s bare `Constant::MessageType::WARNING`
/// must resolve through the full nesting-expansion + alias-edge +
/// qualified-write chain. `bad`'s genuinely undeclared `NOPE` under the
/// resolved target must still accuse (also the negative control below).
#[test]
fn alias_edge_member_resolves_through_full_chain() {
    let diags = check_fixture("alias_edge_resolves.rb");
    assert_eq!(
        diags.len(),
        1,
        "expected exactly 1 diagnostic (only `bad` should accuse), got: {diags:?}"
    );
    assert!(
        diags[0].contains("E0104") && diags[0].contains("NOPE"),
        "`handle`'s bare `Constant::MessageType::WARNING` must resolve through the RBI's own \
         alias edge (`Rbi4wqRubyLsp::Constant = Rbi4wqLanguageServer::Protocol::Constant`); `bad`'s \
         genuinely undeclared `NOPE` must still accuse, got: {diags:?}"
    );
}

/// Sanity re-check of the same fixture's negative half in isolation.
#[test]
fn undeclared_member_under_resolved_target_still_accuses() {
    let diags = check_fixture("alias_edge_resolves.rb");
    assert!(
        diags.iter().any(|d| d.contains("E0104") && d.contains("NOPE")),
        "a member absent from the resolved alias target must still accuse, got: {diags:?}"
    );
}

/// Mutant (c): suppression-only proof. Even resolved through the alias
/// edge, the reference stays `Ty::Unknown` — an obviously undefined
/// method call on it must never raise E0101.
#[test]
fn resolved_alias_edge_still_types_unknown() {
    let diags = check_fixture("alias_edge_resolves.rb");
    assert!(
        !diags.iter().any(|d| d.contains("E0101")),
        "an alias-edge-resolved reference must stay `Ty::Unknown` (suppression-only, invariant \
         #1) — `bad_method_call`'s undefined method call must never raise E0101, got: {diags:?}"
    );
}

/// Mutant (b): the mandatory cycle-guard proof. A pathological `A = B;
/// B = A` pair, both written inside the vendored RBI, must degrade to a
/// silent miss (genuine E0104) — never hang. Bounded by an explicit
/// `recv_timeout`, not just by the whole test binary eventually
/// finishing.
#[test]
fn cycle_never_hangs() {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let diags = check_fixture("cycle_never_hangs.rb");
        let _ = tx.send(diags);
    });
    let diags = rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("an RBI-internal alias cycle must not hang the checker");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(
        diags[0].contains("E0104") && diags[0].contains("RbiY4wqCycleOuter::A::Whatever"),
        "a cyclic RBI alias must degrade to a genuine, silent-miss E0104, got: {diags:?}"
    );
}
