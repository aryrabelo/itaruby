//! Bead ita-uh1: the RBI method closure's method-name hit
//! (`index::rbi_method_lookup`, bead ita-xze) now carries the sorbet
//! return `Ty` the RBI's `sig { ... }` maps to, instead of throwing it
//! away as `Ty::Unknown`. This is what lets a call chain through an
//! external ancestor's declared method CONTINUE instead of dying at the
//! first hit.
//!
//! Every fixture here uses a distinct external ancestor name (`rbi_method_
//! lookup`'s memo is keyed process-wide on the start name alone — see
//! `rbi_methods.rs`'s header comment). Sig-typed fixtures are deliberately
//! toy, not corpus-shaped: the lead measured real `gems/` RBI sig coverage
//! at ~6% (0% in the file that dominates the ancestry-open bucket,
//! activerecord — gem RBIs are reflection-generated, not type-inferred),
//! so a real-corpus expectation would be flaky and wrong to assert on.
//! This file proves the MAPPING contract, not a corpus hit rate.

use std::path::{Path, PathBuf};

use itaruby_semantic::{call_stats, check_file, CallStats, Db, ProjectFiles, RbiProject, SourceFile};

fn tmpdir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sorbet/rbi/gems")).unwrap();
    dir
}

fn wire_rbi(db: &Db, dir: &Path, rbi_file: &str, rbi_text: &str) {
    std::fs::write(dir.join("sorbet/rbi/gems").join(rbi_file), rbi_text).unwrap();
    let files = itaruby_semantic::rbi::discover_rbi_files(&dir.join("sorbet/rbi"));
    let index = itaruby_semantic::rbi::build_rbi_index(&files);
    assert!(
        !index.constants.is_empty(),
        "fixture RBI must index at least one constant"
    );
    RbiProject::new(db, index.constants);
}

fn wire_project(db: &Db, text: &str) -> SourceFile {
    let file = SourceFile::new(db, "inline/rbi_ret_ty.rb".into(), text.to_string());
    ProjectFiles::new(db, vec![file]);
    file
}

fn diag_codes(db: &Db, file: SourceFile) -> Vec<String> {
    check_file(db, file)
        .iter()
        .map(|d| d.code.to_string())
        .collect()
}

/// `write_rbi`'s direct-index-query counterpart (mirrors `rbi_methods.rs`):
/// `project_index` and `rbi_method_lookup` consulted straight, no checker,
/// no `.rbi` written to `sorbet/rbi/gems/` for the *query* map — callers
/// build that map with `write_rbi_map` below and pass it through
/// untouched.
fn write_rbi_map(
    dir: &Path,
    content: &str,
) -> std::collections::HashMap<String, Vec<PathBuf>> {
    let rbi_dir = dir.join("sorbet/rbi/gems");
    std::fs::create_dir_all(&rbi_dir).unwrap();
    std::fs::write(rbi_dir.join("gem.rbi"), content).unwrap();
    let files = itaruby_semantic::rbi::discover_rbi_files(&dir.join("sorbet/rbi"));
    itaruby_semantic::rbi::build_rbi_index(&files).constants
}

fn project_index_for(dir: &Path, text: &str) -> itaruby_semantic::index::ProjectIndex {
    let db = Db::default();
    let file = SourceFile::new(&db, dir.join("project.rb"), text.to_string());
    ProjectFiles::new(&db, vec![file]);
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

/// The headline case: `sig { returns(String) }` on the RBI-declared
/// method makes the call site type `Ty::Str` instead of `Ty::Unknown` —
/// and because it's really `Str`, the very next call on the result
/// (`x.upcase`) resolves through the CORE method table (`Bucket::Core`),
/// not through `Bucket::UnknownReceiver`. That is the whole point of the
/// bead: the chain continues one more hop instead of dying at the RBI
/// hit.
#[test]
fn sig_typed_hit_lets_the_chain_continue() {
    let dir = tmpdir("rbi-ret-ty-sig-hit");
    let db = Db::default();
    wire_rbi(
        &db,
        &dir,
        "sig_hit.rbi",
        "class RbiRetTySigHit::Base\n  sig { returns(String) }\n  def declared_method; end\nend\n",
    );
    let file = wire_project(
        &db,
        "\
class RbiRetTySigHitSite < RbiRetTySigHit::Base
  def initialize; end
end

def use_it
  x = RbiRetTySigHitSite.new.declared_method
  x.upcase
end
",
    );
    let s: CallStats = call_stats(&db, file);
    assert_eq!(
        s.rbi_method, 1,
        "declared_method is the one RBI-declared hit: {s:?}"
    );
    assert_eq!(
        s.core, 1,
        "x.upcase must resolve as a core Str method — proof the hit's Ty::Str propagated: {s:?}"
    );
    assert_eq!(
        s.unknown_receiver, 0,
        "a properly typed chain must never fall into unknown_receiver: {s:?}"
    );
    assert_eq!(
        s.resolved, 1,
        "RbiRetTySigHitSite.new resolves its own initialize: {s:?}"
    );
    assert_eq!(
        s.total(),
        3,
        "new, declared_method, and upcase must all be counted: {s:?}"
    );

    let diags = diag_codes(&db, file);
    assert!(
        diags.is_empty(),
        "a typed RBI hit must never emit a diagnostic on its own: {diags:?}"
    );
}

/// Sig-less hit: the RBI declares the method, but it carries no `sig` at
/// all. The call site is still conclusive (`rbi_method` counts it,
/// exactly like ita-xze's original contract), but the returned `Ty` is
/// `Ty::Unknown`, so the chain dies at the very next call
/// (`Bucket::UnknownReceiver`, not `Bucket::Core`) — same blind spot as
/// before this bead, zero diagnostics either way.
#[test]
fn sig_less_hit_stays_conclusive_but_the_chain_dies_on_unknown() {
    let dir = tmpdir("rbi-ret-ty-no-sig");
    let db = Db::default();
    wire_rbi(
        &db,
        &dir,
        "no_sig.rbi",
        "class RbiRetTyNoSig::Base\n  def declared_method; end\nend\n",
    );
    let file = wire_project(
        &db,
        "\
class RbiRetTyNoSigSite < RbiRetTyNoSig::Base
  def initialize; end
end

def use_it
  x = RbiRetTyNoSigSite.new.declared_method
  x.upcase
end
",
    );
    let s: CallStats = call_stats(&db, file);
    assert_eq!(
        s.rbi_method, 1,
        "declared_method is still a conclusive RBI hit even with no sig: {s:?}"
    );
    assert_eq!(
        s.core, 0,
        "with no sig the result stays Ty::Unknown, so upcase can't resolve as core: {s:?}"
    );
    assert_eq!(
        s.unknown_receiver, 1,
        "upcase on an Unknown receiver lands in unknown_receiver, exactly like before this bead: {s:?}"
    );
    assert_eq!(
        s.inconclusive, 0,
        "the RBI hit itself must never land in inconclusive: {s:?}"
    );

    let diags = diag_codes(&db, file);
    assert!(
        diags.is_empty(),
        "Ty::Unknown never diagnoses, RBI-consulted or not: {diags:?}"
    );
}

/// `T.nilable(String)` maps to a `Union` that contains `Nil` — proven
/// directly against `rbi_method_lookup`'s returned `Ty`, independent of
/// the checker plumbing (same "direct query" pattern `rbi_methods.rs`
/// uses).
#[test]
fn nilable_sig_returns_a_union_containing_nil() {
    let dir = tmpdir("rbi-ret-ty-nilable");
    let map = write_rbi_map(
        &dir,
        "class RbiRetTyNilable::Base\n  sig { returns(T.nilable(String)) }\n  def declared_method; end\nend\n",
    );
    let proj = project_index_for(
        &dir,
        "class RbiRetTyNilableSite < RbiRetTyNilable::Base\nend\n",
    );
    let id = class_id(&proj, "RbiRetTyNilableSite");

    let ty = itaruby_semantic::index::rbi_method_lookup(&proj, id, "declared_method", false, &map)
        .expect("declared_method must be a hit");
    match &ty {
        itaruby_semantic::Ty::Union(members) => {
            assert!(
                members.contains(&itaruby_semantic::Ty::Nil),
                "T.nilable(String) must carry Nil in the union: {ty:?}"
            );
            assert!(
                members.contains(&itaruby_semantic::Ty::Str),
                "T.nilable(String) must carry Str in the union: {ty:?}"
            );
        }
        other => panic!("T.nilable(String) must map to a Union, got {other:?}"),
    }
}

/// Miss stays `None`, unchanged from ita-xze's original contract: no
/// external ancestor in the chain declares the queried name at all.
#[test]
fn miss_stays_none() {
    let dir = tmpdir("rbi-ret-ty-miss");
    let map = write_rbi_map(
        &dir,
        "class RbiRetTyMiss::Base\n  sig { returns(String) }\n  def real_method; end\nend\n",
    );
    let proj = project_index_for(&dir, "class RbiRetTyMissSite < RbiRetTyMiss::Base\nend\n");
    let id = class_id(&proj, "RbiRetTyMissSite");

    assert!(
        itaruby_semantic::index::rbi_method_lookup(&proj, id, "not_declared_anywhere", false, &map)
            .is_none(),
        "a name the RBI never declares must stay a miss"
    );
}

/// Aditive-only lock: a method that exists NOWHERE — not on the project
/// class, not anywhere in the RBI's external ancestor chain (which DOES
/// carry a sig-typed method, to prove typing doesn't change this) —
/// called on a receiver whose ancestry is externally open must never
/// fire E0101. If a future change ever swapped "miss" for
/// `MethodLookup::NotFound`, this is the test that breaks.
#[test]
fn miss_never_manufactures_a_diagnostic_even_with_sigs_present() {
    let dir = tmpdir("rbi-ret-ty-ghost");
    let db = Db::default();
    wire_rbi(
        &db,
        &dir,
        "ghost.rbi",
        "class RbiRetTyGhost::Base\n  sig { returns(String) }\n  def declared_method; end\nend\n",
    );
    let file = wire_project(
        &db,
        "\
class RbiRetTyGhostSite < RbiRetTyGhost::Base
  def initialize; end
end

def use_it
  RbiRetTyGhostSite.new.totally_undeclared_method
end
",
    );
    let diags = diag_codes(&db, file);
    assert!(
        !diags.contains(&"E0101".to_string()),
        "a method missing everywhere must never raise E0101 through an ancestry-open receiver: {diags:?}"
    );
    assert!(
        diags.is_empty(),
        "no other diagnostic should appear either: {diags:?}"
    );
}

/// The corpus-safety guarantee (bead ita-uh1's own acceptance bar): a
/// result typed `Ty::Str` via an RBI sig, calling a method String
/// genuinely does NOT have, must still never diagnose — `ClosedWorld` is
/// off by default (nothing here wires it), so a core receiver's unknown
/// method stays silent exactly like `Ty::Unknown` always did. This is
/// what protects the reference corpora from a new false positive the
/// moment sig-mapped types start flowing.
#[test]
fn typed_result_with_undeclared_core_method_stays_silent() {
    let dir = tmpdir("rbi-ret-ty-core-miss");
    let db = Db::default();
    wire_rbi(
        &db,
        &dir,
        "core_miss.rbi",
        "class RbiRetTyCoreMiss::Base\n  sig { returns(String) }\n  def declared_method; end\nend\n",
    );
    let file = wire_project(
        &db,
        "\
class RbiRetTyCoreMissSite < RbiRetTyCoreMiss::Base
  def initialize; end
end

def use_it
  x = RbiRetTyCoreMissSite.new.declared_method
  x.not_a_real_string_method
end
",
    );
    let s: CallStats = call_stats(&db, file);
    assert_eq!(
        s.rbi_method, 1,
        "declared_method is still the one RBI hit: {s:?}"
    );

    let diags = diag_codes(&db, file);
    assert!(
        diags.is_empty(),
        "a Ty::Str receiver calling an undeclared method must stay silent with ClosedWorld off: {diags:?}"
    );
}
