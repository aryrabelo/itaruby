//! Bead ita-xze (extended by ita-uh1): call-site escalation through
//! Tapioca RBIs (`index::rbi_method_lookup`). `check.rs`'s
//! `Checker::rbi_escalate` consults it exactly once a
//! `MethodLookup::Inconclusive` for a project-class receiver has already
//! given up — a project class whose superclass never resolves in the
//! project's own index (an EXTERNAL ancestor), where the client's Tapioca
//! RBI declares the method being called. A hit moves the call site from
//! `inconclusive` (blind) to the new `rbi_method` bucket; a miss changes
//! nothing (invariant #1: never a diagnostic either way, `Inconclusive`
//! never diagnosed before this bead and still doesn't). None of the
//! toy RBIs below carry a sorbet sig, so every hit here types
//! `Ty::Unknown` — the sig-mapping contract itself (ita-uh1) lives in
//! `rbi_ret_ty.rs`; this file only proves the hit/miss bucketing and the
//! silence guarantee still hold.
//!
//! Fixtures are synthetic, one-off `sorbet/rbi/gems/*.rbi` files under a
//! `CARGO_TARGET_TMPDIR` tempdir (never `testdata/` — a corpus-scan root
//! would leak these synthetic classes across every other fixture file),
//! same discovery shape `rbi_ancestry.rs` uses for the constant-lookup
//! side of this same RBI channel. Project sources stay inline text
//! (`SourceFile::new` never touches the filesystem for those) — only the
//! RBI itself needs real files, since `rbi_method_lookup`'s BFS reads
//! and parses them lazily off disk.
//!
//! Every fixture below gives its project class its OWN `initialize`, so
//! `ClassName.new` always resolves `Bucket::Resolved` and contributes no
//! `Inconclusive` noise of its own — the one call site under test is the
//! only ancestry-open lookup in each source, keeping every count exact.
//! Every external ancestor name is unique per test: `rbi_method_lookup`
//! memoizes its BFS process-wide, keyed only by that name (not by which
//! `rbi_map` supplied it), so two tests sharing a name inside the same
//! test binary could otherwise read each other's fixture.

use std::path::{Path, PathBuf};

use itaruby_semantic::{
    call_stats, check_file, CallStats, Db, ProjectFiles, RbiProject, SourceFile,
};

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
    let file = SourceFile::new(db, "inline/rbi_call_resolution.rb".into(), text.to_string());
    ProjectFiles::new(db, vec![file]);
    file
}

fn diag_codes(db: &Db, file: SourceFile) -> Vec<String> {
    check_file(db, file)
        .iter()
        .map(|d| d.code.to_string())
        .collect()
}

/// The headline case: `RbiXzeHitSite < RbiXzeHit::Base` (an unresolved
/// external superclass) calls `declared_method`, which only `RbiXzeHit::
/// Base`'s Tapioca RBI declares. `new` resolves cleanly (project-defined
/// `initialize`); `declared_method` is the one Inconclusive lookup, and
/// the RBI hit makes it conclusive: `rbi_method` counts it, no
/// diagnostic, and `total()` still accounts for both call sites.
#[test]
fn instance_method_hit_is_conclusive_and_silent() {
    let dir = tmpdir("rbi-xze-hit");
    let db = Db::default();
    wire_rbi(
        &db,
        &dir,
        "hit.rbi",
        "class RbiXzeHit::Base\n  def declared_method; end\nend\n",
    );
    let file = wire_project(
        &db,
        "\
class RbiXzeHitSite < RbiXzeHit::Base
  def initialize; end
end

def use_it
  RbiXzeHitSite.new.declared_method
end
",
    );
    let s: CallStats = call_stats(&db, file);
    assert_eq!(
        s.rbi_method, 1,
        "declared_method is RBI-declared on the external ancestor: {s:?}"
    );
    assert_eq!(
        s.inconclusive, 0,
        "the RBI hit must be counted OUT of inconclusive: {s:?}"
    );
    assert_eq!(
        s.resolved, 1,
        "RbiXzeHitSite.new resolves its own initialize: {s:?}"
    );
    assert_eq!(
        s.total(),
        2,
        "both call sites (new, declared_method) must still sum to total(): {s:?}"
    );

    let diags = diag_codes(&db, file);
    assert!(
        diags.is_empty(),
        "an RBI hit must never emit a diagnostic: {diags:?}"
    );
}

/// Same shape, method the RBI does NOT declare: the call stays exactly
/// where it was before this bead — `inconclusive`, silent, `rbi_method`
/// untouched.
#[test]
fn instance_method_miss_stays_inconclusive_and_silent() {
    let dir = tmpdir("rbi-xze-miss");
    let db = Db::default();
    wire_rbi(
        &db,
        &dir,
        "miss.rbi",
        "class RbiXzeMiss::Base\n  def declared_method; end\nend\n",
    );
    let file = wire_project(
        &db,
        "\
class RbiXzeMissSite < RbiXzeMiss::Base
  def initialize; end
end

def use_it
  RbiXzeMissSite.new.absent_method
end
",
    );
    let s: CallStats = call_stats(&db, file);
    assert_eq!(
        s.rbi_method, 0,
        "absent_method is not declared anywhere in the RBI: {s:?}"
    );
    assert_eq!(
        s.inconclusive, 1,
        "the miss must still land in inconclusive: {s:?}"
    );
    assert_eq!(
        s.resolved, 1,
        "RbiXzeMissSite.new resolves its own initialize: {s:?}"
    );
    assert_eq!(
        s.total(),
        2,
        "both call sites must still sum to total(): {s:?}"
    );

    let diags = diag_codes(&db, file);
    assert!(
        diags.is_empty(),
        "Inconclusive never diagnoses, RBI-consulted or not: {diags:?}"
    );
}

/// Regression lock for the aditive-only contract itself (never touch
/// `index.rs`/`rbi_method_lookup` to make a miss produce `NotFound`):
/// a method that exists NOWHERE — not on the project class, not on any
/// RBI — called on a receiver whose ancestry is externally open must
/// never fire E0101. If a future change ever swapped "miss" for
/// `MethodLookup::NotFound`, this is the test that breaks.
#[test]
fn miss_never_manufactures_a_diagnostic() {
    let dir = tmpdir("rbi-xze-ghost");
    let db = Db::default();
    wire_rbi(
        &db,
        &dir,
        "ghost.rbi",
        "class RbiXzeGhost::Base\n  def declared_method; end\nend\n",
    );
    let file = wire_project(
        &db,
        "\
class RbiXzeGhostSite < RbiXzeGhost::Base
  def initialize; end
end

def use_it
  RbiXzeGhostSite.new.totally_undeclared_method
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

/// Singleton/class-method side: `RbiXzeSingletonSite.declared_class_method`
/// resolves `lookup_singleton`, Inconclusive because the superclass is
/// external — the RBI's `self.declared_class_method` closes it the same
/// way, into the SAME `rbi_method` bucket (no separate singleton bucket).
#[test]
fn singleton_method_hit_counts_in_the_same_bucket() {
    let dir = tmpdir("rbi-xze-singleton");
    let db = Db::default();
    wire_rbi(
        &db,
        &dir,
        "singleton.rbi",
        "class RbiXzeSingleton::Base\n  def self.declared_class_method; end\nend\n",
    );
    let file = wire_project(
        &db,
        "\
class RbiXzeSingletonSite < RbiXzeSingleton::Base
  def initialize; end
end

def use_it
  RbiXzeSingletonSite.declared_class_method
end
",
    );
    let s: CallStats = call_stats(&db, file);
    assert_eq!(
        s.rbi_method, 1,
        "declared_class_method is RBI-declared as a singleton method: {s:?}"
    );
    assert_eq!(
        s.inconclusive, 0,
        "the hit must be counted OUT of inconclusive: {s:?}"
    );
    assert_eq!(
        s.total(),
        1,
        "the single class-method call site must sum to total(): {s:?}"
    );

    let diags = diag_codes(&db, file);
    assert!(
        diags.is_empty(),
        "a singleton RBI hit must never emit a diagnostic: {diags:?}"
    );
}

/// Cost proof: a project with NO `sorbet/rbi` wired at all (`RbiProject`
/// never set — `rbi_map` is `None`) must behave byte-for-byte like
/// before this bead. Same shape as the instance-method miss fixture
/// (same call-site count, same buckets), minus any RBI file or wiring —
/// `Checker::rbi_escalate` exits on the `Option` check before touching
/// `index::rbi_method_lookup` at all.
#[test]
fn project_without_rbi_pays_zero_and_matches_pre_bead_counts() {
    let db = Db::default();
    let file = wire_project(
        &db,
        "\
class RbiXzeNoRbiSite < RbiXzeNoRbi::Base
  def initialize; end
end

def use_it
  RbiXzeNoRbiSite.new.declared_method
end
",
    );
    let s: CallStats = call_stats(&db, file);
    assert_eq!(
        s.rbi_method, 0,
        "no RbiProject wired means zero rbi_method hits, ever: {s:?}"
    );
    assert_eq!(
        s.inconclusive, 1,
        "declared_method stays exactly where it was before this bead: {s:?}"
    );
    assert_eq!(
        s.resolved, 1,
        "RbiXzeNoRbiSite.new still resolves its own initialize: {s:?}"
    );
    assert_eq!(
        s.total(),
        2,
        "both call sites must still sum to total(): {s:?}"
    );

    let diags = diag_codes(&db, file);
    // The unresolved superclass CONSTANT itself still warns E0104 (no
    // RBI to resolve `RbiXzeNoRbi::Base` with) — that is `check_const_ref`'s
    // pre-existing, unrelated contract (see `rbi_ancestry.rs`'s own
    // `without_rbi_the_same_reference_still_warns`). The METHOD call
    // through that same open ancestor must stay silent regardless: no
    // E0101, and nothing else.
    assert_eq!(
        diags,
        vec!["E0104".to_string()],
        "only the superclass const warns, never the method call: {diags:?}"
    );
}
