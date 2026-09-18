//! W3 require/autoload: external (Tapioca RBI) ancestry for constant
//! lookup (`index.rs::rbi_ancestor_declares`). Fixtures live in
//! `testdata/rbi_ancestry/`: a synthetic Tapioca-style RBI under
//! `sorbet/rbi/gems/` declares the gem-side superclass chain and the
//! mixin whose constants the app references (`probe.rb`), plus a control
//! (`control.rb`) proving a constant no RBI declares keeps warning. The
//! RBI is wired explicitly here (the nested `sorbet/rbi` is NOT
//! discovered when checking `testdata/` as one root — per-root
//! discovery, same reason `core_conclusive/with_gemfile/` fires on
//! purpose in gate c).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn fixture_dir(rel: &str) -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/rbi_ancestry")).join(rel)
}

fn check_with_rbi(name: &str, wire_rbi: bool) -> Vec<(String, String)> {
    let dir = fixture_dir(".");
    let rbi_dir = dir.join("sorbet/rbi");
    let path = dir.join(name);
    let text = std::fs::read_to_string(&path).unwrap();
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, path, text.clone());
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    if wire_rbi {
        let files = itaruby_semantic::rbi::discover_rbi_files(&rbi_dir);
        let index = itaruby_semantic::rbi::build_rbi_index(&files);
        assert!(!index.constants.is_empty(), "fixture RBI must index");
        itaruby_semantic::RbiProject::new(&db, index.constants);
    }
    itaruby_semantic::check_file(&db, itaruby_semantic::ProjectFiles::try_get(&db).unwrap().files(&db)[0])
        .iter()
        .map(|d| (d.code.to_string(), d.message.clone()))
        .collect()
}

/// The headline case: a bare constant that exists only on the gem mixin
/// behind an unresolved superclass chain resolves once the RBI is wired —
/// E0104 suppressed, and nothing else appears (ancestry stays unknown, so
/// invariant #1 holds trivially).
#[test]
fn mixin_constant_behind_external_superclass_resolves() {
    let diags = check_with_rbi("probe.rb", true);
    assert!(
        diags.is_empty(),
        "IDENT lives on SynthGem::TypeNames, mixed into the superclass chain the RBI declares; expected silence, got: {diags:?}"
    );
}

/// Same file, RBI absent: the old behavior — E0104 for the unresolved
/// superclass AND for IDENT (the `ClassNode` arm checks superclasses too).
#[test]
fn without_rbi_the_same_reference_still_warns() {
    let diags = check_with_rbi("probe.rb", false);
    let msgs: Vec<&str> = diags.iter().map(|(_, m)| m.as_str()).collect();
    assert_eq!(diags.len(), 2, "expected superclass + IDENT warnings, got: {msgs:?}");
    assert!(
        msgs.iter().any(|m| m.contains("SynthGem::Schema::Object")),
        "superclass must warn without the RBI: {msgs:?}"
    );
    assert!(
        msgs.iter().any(|m| m.contains("IDENT")),
        "IDENT must warn without the RBI: {msgs:?}"
    );
}

/// Control: a constant no RBI declares keeps warning even with the RBI
/// wired — the external walk resolves names, it never blankets them.
#[test]
fn unknown_constant_keeps_warning_with_rbi_wired() {
    let diags = check_with_rbi("control.rb", true);
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].0 == "E0104", "expected E0104, got: {:?}", diags[0]);
    assert!(diags[0].1.contains("NotInAnyRbiConst"), "got: {:?}", diags[0].1);
}

/// Mutation guard for the qualified-write harvester: Tapioca's
/// `A::B::C = T.let(...)` form (a `ConstantPathWriteNode`, not the simple
/// `ConstantWriteNode`) must land in `FileDefs::consts` — this is the
/// exact shape the graphql RBI's mixin constants use, and the walk
/// silently ignored it before W3.
#[test]
fn tapioca_qualified_const_write_is_harvested() {
    let text = "SynthGem::TypeNames::IDENT = T.let(T.unsafe(nil), String)\n";
    let defs = itaruby_semantic::index::parse_defs_text(text);
    assert!(
        defs.consts.iter().any(|(n, ..)| n == "SynthGem::TypeNames::IDENT"),
        "qualified write must be harvested into consts, got: {:?}",
        defs.consts
    );
}

/// The external closure walks superclass AND include edges declared by
/// the RBI itself — Object -> Member -> `TypeNames` — and collects the
/// qualified constants at each hop. Direct assertion on the query,
/// independent of the checker plumbing.
#[test]
fn closure_collects_mixin_constants_through_superclass_chain() {
    let rbi_dir: &Path = &fixture_dir("sorbet/rbi");
    let files = itaruby_semantic::rbi::discover_rbi_files(rbi_dir);
    let index = itaruby_semantic::rbi::build_rbi_index(&files);
    let map: HashMap<String, Vec<PathBuf>> = index.constants;
    // Start where the PROJECT index stops: the unresolved superclass.
    let db = itaruby_semantic::Db::default();
    let text = "class RbiAncUsesMixin < SynthGem::Schema::Object\n  def m(x)\n    IDENT\n  end\nend\n";
    let file = itaruby_semantic::SourceFile::new(
        &db,
        fixture_dir("inline.rb"),
        text.to_string(),
    );
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    let proj = itaruby_semantic::project_index(&db);
    assert!(
        itaruby_semantic::index::rbi_ancestor_declares(proj, &["RbiAncUsesMixin".to_string()], "IDENT", &map),
        "IDENT must resolve through Object -> Member -> TypeNames"
    );
    assert!(
        !itaruby_semantic::index::rbi_ancestor_declares(proj, &["RbiAncUsesMixin".to_string()], "NotInAnyRbiConst", &map),
        "a name the chain never defines must not resolve"
    );
}

/// Bead ita-dpg.1, the regression this file exists to keep dead: the
/// gem-side superclass RESOLVES in the project index (it is one of the
/// names `declarations/rbs_collection.rbi` declares, force-opened as
/// `OpenReason::DeclaredExternal`), so `unresolved_ancestors` never
/// reports it and an RBI walk keyed on that population alone never
/// starts. `external_lookup_starts` must use `external_ancestor_starts`'
/// union instead. Mutant: swap the two calls in that function back to
/// `unresolved_ancestors` — this test fails while every other test in
/// this file stays green (they exercise the unresolved-name population,
/// which the mutant leaves intact), and corpus-c's E0104 ceiling goes
/// 179 -> 2746.
#[test]
fn constant_behind_declared_external_superclass_resolves() {
    let diags = check_with_rbi("probe_declared_superclass.rb", true);
    assert!(
        diags.is_empty(),
        "SYNTHIDENT lives on the mixin the RBI puts behind a DECLARED superclass; expected silence, got: {diags:?}"
    );
}

/// Control for the test above: with no RBI wired, the same reference
/// keeps warning — the declared superclass alone never resolves a
/// constant, so the fix widens WHERE the RBI walk starts and nothing
/// else. Exactly one warning, not two: unlike `probe.rb`, the superclass
/// here is a declared name that resolves, so only `SYNTHIDENT` is left.
#[test]
fn without_rbi_the_declared_superclass_reference_still_warns() {
    let diags = check_with_rbi("probe_declared_superclass.rb", false);
    let msgs: Vec<&str> = diags.iter().map(|(_, m)| m.as_str()).collect();
    assert_eq!(diags.len(), 1, "expected only the SYNTHIDENT warning, got: {msgs:?}");
    assert!(msgs[0].contains("SYNTHIDENT"), "expected SYNTHIDENT, got: {msgs:?}");
}
