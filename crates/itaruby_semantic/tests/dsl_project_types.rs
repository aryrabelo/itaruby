//! Bead ita-tjr: the DSL-RBI project-type layer, built on top of the two
//! pipelines the session already shipped — `sorbet_sig::sorbet_ret_ty`
//! (bead ita-uh1, index-blind sig-text -> `Ty` mapping) and
//! `index::rbi_method_lookup`'s external-ancestor RBI walk (bead ita-xze).
//! Two independent groups:
//!
//! 1. `sorbet_sig::resolve_ret_ty` (pure `Option<&str>` + `ProjectIndex`
//!    -> `Ty`, no checker involved): the index-aware layer that resolves a
//!    bare project-class leaf to `Ty::Instance`, while `sorbet_ret_ty`
//!    itself must keep NEVER doing so — that split is this bead's whole
//!    safety argument (see `sorbet_sig.rs`'s module doc comment).
//! 2. The `check.rs` wiring: `Checker::rbi_escalate` now tries
//!    `index::dsl_method_lookup` (a Tapioca `dsl/` RBI reopening the
//!    receiver's OWN project class) before `index::rbi_method_lookup` (an
//!    external ancestor's RBI) — disjoint buckets, `dsl_method` vs
//!    `rbi_method`, and this is aditive-only: it never closes a class, so
//!    every fixture pairs a DSL RBI reopening with an ordinary open
//!    ancestry (an unresolved external superclass), matching real Tapioca
//!    corpora where the class is already open for another reason.
//!
//! Every fixture uses a name unique to this binary (matching
//! `tests/rbi_methods.rs`'s discipline): `rbi_method_closure`'s/its DSL
//! counterpart's memo is keyed on the start name alone, process-wide, so
//! two tests sharing an external ancestor or reopened class name would
//! serve each other's cached result and fail by run order, not by
//! behavior. Fixtures live in a per-test tempdir
//! (`std::env::temp_dir()`), never `testdata/` — the corpora are
//! read-only client code and `ita check testdata/` scans the whole tree
//! as one project.

use std::path::{Path, PathBuf};

use itaruby_semantic::index::ProjectIndex;
use itaruby_semantic::sorbet_sig::{resolve_ret_ty, sorbet_ret_ty};
use itaruby_semantic::types::{ClassId, Ty};
use itaruby_semantic::{call_stats, check_file, CallStats, Db, ProjectFiles, RbiProject, SourceFile};

// ---------------------------------------------------------------------------
// Group 1: `resolve_ret_ty` — pure mapping, direct `ProjectIndex` query.
// ---------------------------------------------------------------------------

fn project_index_for(dir: &Path, text: &str) -> ProjectIndex {
    let db = Db::default();
    let file = SourceFile::new(&db, dir.join("project.rb"), text.to_string());
    ProjectFiles::new(&db, vec![file]);
    itaruby_semantic::project_index(&db).clone()
}

fn class_id(index: &ProjectIndex, path: &str) -> ClassId {
    *index
        .by_path
        .get(path)
        .unwrap_or_else(|| panic!("{path} must be interned in the project index"))
}

fn sig_tmpdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "itaruby-dsl-project-types-sig-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The headline resolution: a bare project-class name in sig text becomes
/// `Ty::Instance` of that class's real `ClassId`.
#[test]
fn resolves_project_class_name_to_instance() {
    let dir = sig_tmpdir("bare-name");
    let index = project_index_for(&dir, "class DslRetTyQuantity\nend\n");
    let id = class_id(&index, "DslRetTyQuantity");

    assert_eq!(
        resolve_ret_ty(Some("DslRetTyQuantity"), &index),
        Ty::Instance(id)
    );
}

/// Aninhamento: `T.nilable(::DslRetTyQuantity)` must resolve the INNER
/// name — the leaf, not the whole expression — so the result is a
/// `Union` that actually contains `Ty::Instance`, not `Unknown`.
#[test]
fn resolves_project_class_inside_nilable() {
    let dir = sig_tmpdir("nilable");
    let index = project_index_for(&dir, "class DslRetTyNilableQuantity\nend\n");
    let id = class_id(&index, "DslRetTyNilableQuantity");

    let ty = resolve_ret_ty(Some("T.nilable(::DslRetTyNilableQuantity)"), &index);
    assert_eq!(ty, Ty::union(Ty::Instance(id), Ty::Nil));
    match ty {
        Ty::Union(parts) => assert!(
            parts.contains(&Ty::Instance(id)),
            "union must actually contain the resolved Instance: {parts:?}"
        ),
        other => panic!("expected a Union, got {other:?}"),
    }
}

/// Same leaf-resolution property through `T::Array[...]`.
#[test]
fn resolves_project_class_inside_array() {
    let dir = sig_tmpdir("array");
    let index = project_index_for(&dir, "class DslRetTyArrayQuantity\nend\n");
    let id = class_id(&index, "DslRetTyArrayQuantity");

    assert_eq!(
        resolve_ret_ty(Some("T::Array[DslRetTyArrayQuantity]"), &index),
        Ty::Array(Box::new(Ty::Instance(id)))
    );
}

/// A name that looks exactly like a project class but never resolves in
/// THIS index (no such class defined) must stay `Ty::Unknown` — never a
/// chute.
#[test]
fn unresolved_name_stays_unknown() {
    let dir = sig_tmpdir("unresolved");
    let index = project_index_for(&dir, "class DslRetTyUnrelated\nend\n");

    assert_eq!(
        resolve_ret_ty(Some("DslRetTyNeverDefinedAnywhere"), &index),
        Ty::Unknown
    );
}

/// `None` (no captured sig text at all) is `Ty::Unknown`.
#[test]
fn none_is_unknown() {
    let dir = sig_tmpdir("none");
    let index = project_index_for(&dir, "class DslRetTyNoneProbe\nend\n");

    assert_eq!(resolve_ret_ty(None, &index), Ty::Unknown);
}

/// Scalars still flow through the SAME lower layer `sorbet_ret_ty` uses
/// (`String` -> `Str`), and — the central safety property this bead
/// rests on — `sorbet_ret_ty` itself, called on the exact same text that
/// `resolve_ret_ty` maps to an `Instance` above, must keep NEVER
/// producing `Instance`/`Class`: it has no index. Both assertions live in
/// one test to make the layering explicit.
#[test]
fn scalars_work_and_the_index_blind_layer_never_produces_instance() {
    let dir = sig_tmpdir("layering");
    let index = project_index_for(&dir, "class DslRetTyLayeringQuantity\nend\n");

    assert_eq!(resolve_ret_ty(Some("String"), &index), Ty::Str);

    assert_eq!(
        resolve_ret_ty(Some("DslRetTyLayeringQuantity"), &index),
        Ty::Instance(class_id(&index, "DslRetTyLayeringQuantity"))
    );
    assert_eq!(
        sorbet_ret_ty("DslRetTyLayeringQuantity"),
        Ty::Unknown,
        "the index-blind function must never guess a project class name into Instance/Class"
    );
}

// ---------------------------------------------------------------------------
// Group 2: `check.rs` wiring — `dsl_method_lookup` before `rbi_method_lookup`.
// ---------------------------------------------------------------------------

fn wire_tmpdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "itaruby-dsl-project-types-wire-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sorbet/rbi/dsl")).unwrap();
    std::fs::create_dir_all(dir.join("sorbet/rbi/gems")).unwrap();
    dir
}

/// Writes every `(relative-path-under-sorbet/rbi, content)` pair, then
/// discovers + indexes the whole `sorbet/rbi` tree ONCE and wires the
/// single `RbiProject` singleton input — mirrors `tests/rbi_ret_ty.rs`'s
/// `wire_rbi`, generalized to more than one file so a DSL fixture and a
/// gems fixture can coexist in the same wired project.
fn wire_rbi_files(db: &Db, dir: &Path, files: &[(&str, &str)]) {
    for (rel, content) in files {
        let full = dir.join("sorbet/rbi").join(rel);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(&full, content).unwrap();
    }
    let discovered = itaruby_semantic::rbi::discover_rbi_files(&dir.join("sorbet/rbi"));
    let index = itaruby_semantic::rbi::build_rbi_index(&discovered);
    assert!(
        !index.constants.is_empty(),
        "fixture RBI(s) must index at least one constant"
    );
    RbiProject::new(db, index.constants);
}

fn wire_project(db: &Db, dir: &Path, text: &str) -> SourceFile {
    let file = SourceFile::new(db, dir.join("project.rb"), text.to_string());
    ProjectFiles::new(db, vec![file]);
    file
}

fn diag_codes(db: &Db, file: SourceFile) -> Vec<String> {
    check_file(db, file)
        .iter()
        .map(|d| d.code.to_string())
        .collect()
}

/// The wiring test: one project with two open-ancestry classes.
/// `DslWireQuantity` is reopened DIRECTLY by a `dsl/` RBI (Tapioca's own
/// pattern: the app model's own class, per-app-model file) — that hit
/// must land in `dsl_method`, NOT `rbi_method`. `DslWireGemSite` instead
/// inherits an UNRESOLVED external name whose RBI lives in `gems/` — that
/// hit is the ita-xze population and must land in `rbi_method`, NOT
/// `dsl_method`. Both classes stay open (an unresolved external
/// superclass each) exactly like real Tapioca-covered `ActiveRecord`
/// models — this bead is additive-only, it never closes anything.
#[test]
fn dsl_hit_and_external_hit_land_in_disjoint_buckets_and_sum_to_total() {
    let dir = wire_tmpdir("disjoint-buckets");
    let db = Db::default();
    wire_rbi_files(
        &db,
        &dir,
        &[
            (
                "dsl/dsl_wire_quantity.rbi",
                "class DslWireQuantity\n  sig { returns(String) }\n  def dsl_col; end\nend\n",
            ),
            (
                "gems/dsl_wire_gem.rbi",
                "class DslWireGemBase\n  def gem_method; end\nend\n",
            ),
        ],
    );
    let file = wire_project(
        &db,
        &dir,
        "\
class DslWireQuantity < ActiveRecord::Base
  def initialize; end
end

class DslWireGemSite < DslWireGemBase
  def initialize; end
end

def use_it
  DslWireQuantity.new.dsl_col
  DslWireGemSite.new.gem_method
end
",
    );

    let s: CallStats = call_stats(&db, file);
    assert_eq!(
        s.dsl_method, 1,
        "the DSL RBI reopening DslWireQuantity itself must hit dsl_method: {s:?}"
    );
    assert_eq!(
        s.rbi_method, 1,
        "the external-ancestor gems/ RBI hit must hit rbi_method, not dsl_method: {s:?}"
    );
    assert_eq!(
        s.total(),
        s.resolved
            + s.core
            + s.rbi_method
            + s.dsl_method
            + s.inconclusive
            + s.unknown_receiver
            + s.diagnosed,
        "total() must sum every top-level bucket exactly once: {s:?}"
    );
    assert_eq!(
        s.total(),
        4,
        "two `.new`s (resolved) plus the two dsl_col/gem_method hits: {s:?}"
    );

    assert!(
        diag_codes(&db, file).is_empty(),
        "two conclusive RBI hits must never emit a diagnostic on their own"
    );
}

/// The point of the bead: a DSL RBI method typed via `resolve_ret_ty`
/// keeps the chain alive. `DslWireChainHost` (open via an unresolved
/// external superclass) is reopened by a `dsl/` RBI whose `sig {
/// returns(...) }` names ANOTHER project class, `DslWireChainQuantity`.
/// The call site must resolve `.chain_col` through `dsl_method`, and the
/// SUBSEQUENT call `.real_method` — a REAL method defined on
/// `DslWireChainQuantity`'s own project body — must resolve too, proving
/// the returned `Ty::Instance` actually carries the right `ClassId`
/// through to the next hop instead of dying on `Ty::Unknown`.
///
/// The sig spells the class absolutely, as Tapioca emits it: a BARE name
/// inside a class whose ancestry is opaque (the unresolved gem superclass
/// could carry a same-named constant) is `Unknown` by design — see
/// `inherited_namespace_constants_never_bind_to_top_level`.
#[test]
fn dsl_sig_returning_a_project_class_lets_the_chain_continue() {
    let dir = wire_tmpdir("chain-continues");
    let db = Db::default();
    wire_rbi_files(
        &db,
        &dir,
        &[(
            "dsl/dsl_wire_chain_host.rbi",
            "class DslWireChainHost\n  sig { returns(::DslWireChainQuantity) }\n  def chain_col; end\nend\n",
        )],
    );
    let file = wire_project(
        &db,
        &dir,
        "\
class DslWireChainHost < ActiveRecord::Base
  def initialize; end
end

class DslWireChainQuantity
  def real_method
    1
  end
end

def use_it
  x = DslWireChainHost.new.chain_col
  x.real_method
end
",
    );

    let s: CallStats = call_stats(&db, file);
    assert_eq!(
        s.dsl_method, 1,
        "chain_col must resolve through the DSL RBI hit: {s:?}"
    );
    assert_eq!(
        s.resolved, 2,
        "DslWireChainHost.new (project initialize) and x.real_method (typed via the DSL sig's \
         resolved Instance) must both resolve as project methods: {s:?}"
    );
    assert_eq!(
        s.unknown_receiver, 0,
        "a properly resolved Instance must never fall into unknown_receiver: {s:?}"
    );

    assert!(
        diag_codes(&db, file).is_empty(),
        "a correctly typed chain must never emit a diagnostic on its own"
    );
}

/// Aditive-only lock: a method that exists NOWHERE — not on the project
/// class body, not in the DSL RBI that reopens the SAME class (which
/// does declare a differently-named method, to prove this isn't a weak
/// "empty RBI" miss) — called on a class open via an unresolved external
/// superclass must never fire E0101. If a future change ever closed this
/// class or converted the miss into `MethodLookup::NotFound`, this is the
/// test that breaks.
#[test]
fn missing_method_on_dsl_reopened_class_never_manufactures_a_diagnostic() {
    let dir = wire_tmpdir("lock-additive-only");
    let db = Db::default();
    wire_rbi_files(
        &db,
        &dir,
        &[(
            "dsl/dsl_wire_lock_host.rbi",
            "class DslWireLockHost\n  sig { returns(String) }\n  def declared_col; end\nend\n",
        )],
    );
    let file = wire_project(
        &db,
        &dir,
        "\
class DslWireLockHost < ActiveRecord::Base
  def initialize; end
end

def use_it
  DslWireLockHost.new.this_method_exists_nowhere_at_all
end
",
    );

    let diags = diag_codes(&db, file);
    assert!(
        diags.is_empty(),
        "a method missing from both the project class and its DSL RBI must stay silent, \
         never E0101: {diags:?}"
    );

    let s: CallStats = call_stats(&db, file);
    assert_eq!(
        s.diagnosed, 0,
        "no call site here may ever land in the diagnosed bucket: {s:?}"
    );
}
