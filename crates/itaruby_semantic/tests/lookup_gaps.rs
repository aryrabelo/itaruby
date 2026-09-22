//! Two NotFound-softening lookup gaps unmasked by bead ita-4xy's `sig{}`
//! open-class carve-out (a class that used to stay `open` on a recognized
//! `sig { ... }`/`def` pair now closes, which surfaced 63 corpus false
//! `E0101`s the lead review classified as FP — see `AGENTS.md`).
//! `ProjectIndex::lookup_method`/`lookup_singleton`'s own walk (unchanged)
//! still commits to `NotFound` in both cases; `lookup_method_rbi`/
//! `lookup_singleton_rbi` (`index.rs`) intercept that `NotFound` and soften
//! it to `Inconclusive` when the "closed" verdict is provably only a
//! PARTIAL view:
//!
//! 1. Gem-reopening: the project reopens a class (`class ::Foo`) whose
//!    PRIMARY definition lives in a gem — the client's Tapioca RBI also
//!    declares the same fully-qualified path (`gem_reopens`,
//!    `soften_not_found`). Typed resolution, when the RBI's sig maps one,
//!    flows through the EXISTING `rbi_method_lookup`/`rbi_escalate` walk
//!    (`external_ancestor_starts` now also seeds `id`'s own path).
//! 2. Kernel/Object: a bare self-send of a `Kernel` private method
//!    (`proc`, `lambda`, ...) or an `Object`/`BasicObject` (singleton
//!    side: also `Class`/`Module`) method is legitimate on ANY receiver,
//!    closed project class or not (`core::kernel_object_instance_method`/
//!    `core::kernel_object_singleton_method`).
//!
//! Every fixture below uses neutral names (`LkupGap*`), never a real
//! corpus class name. Direct `ProjectIndex` assertions (mirroring
//! `dynamic_include.rs`) pin the fix at the layer that actually decides
//! `Found`/`NotFound`/`Inconclusive`; the gem-reopening tests additionally
//! go through `check_file`/`call_stats` end to end (mirroring
//! `rbi_call_resolution.rs`) since that gap's typed-resolution reuse
//! depends on the full `Checker::rbi_escalate` wiring, not just the
//! lookup call itself.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use itaruby_semantic::index::MethodLookup;
use itaruby_semantic::types::ClassId;
use itaruby_semantic::{call_stats, check_file, CallStats, Db, ProjectFiles, RbiProject, SourceFile};

fn tmpdir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sorbet/rbi/gems")).unwrap();
    dir
}

/// One project file's `ProjectIndex`, independent of the checker plumbing
/// — same "direct query" pattern `rbi_methods.rs`/`dynamic_include.rs` use.
fn project_index_for(text: &str) -> itaruby_semantic::ProjectIndex {
    let db = Db::default();
    let file = SourceFile::new(&db, "inline/lookup_gaps.rb".into(), text.to_string());
    ProjectFiles::new(&db, vec![file]);
    itaruby_semantic::project_index(&db).clone()
}

fn class_id(index: &itaruby_semantic::ProjectIndex, path: &str) -> ClassId {
    *index
        .by_path
        .get(path)
        .unwrap_or_else(|| panic!("class `{path}` not indexed"))
}

fn wire_rbi(dir: &Path, rbi_file: &str, rbi_text: &str) -> HashMap<String, Vec<PathBuf>> {
    std::fs::write(dir.join("sorbet/rbi/gems").join(rbi_file), rbi_text).unwrap();
    let files = itaruby_semantic::rbi::discover_rbi_files(&dir.join("sorbet/rbi"));
    let index = itaruby_semantic::rbi::build_rbi_index(&files);
    assert!(
        !index.constants.is_empty(),
        "fixture RBI must index at least one constant"
    );
    index.constants
}

fn wire_project(db: &Db, text: &str) -> SourceFile {
    let file = SourceFile::new(db, "inline/lookup_gaps.rb".into(), text.to_string());
    ProjectFiles::new(db, vec![file]);
    file
}

fn diag_codes(db: &Db, file: SourceFile) -> Vec<String> {
    check_file(db, file)
        .iter()
        .map(|d| d.code.to_string())
        .collect()
}

// -- (1) guardian: a real typo on a closed class must still diagnose ------

/// The regression lock the whole bead is graded against: neither fallback
/// may soften a GENUINE unknown method on a closed class with no gem
/// reopening and no Kernel/Object name involved. If this ever goes
/// silent, one of the two fallbacks over-fired.
#[test]
fn closed_class_real_typo_still_diagnoses() {
    let index = project_index_for(
        "\
class LkupGapClosedReal
  sig { void }
  def known_method
  end

  def caller_method
    totally_unknown_gap_typo_zzz
  end
end
",
    );
    let id = class_id(&index, "LkupGapClosedReal");
    assert!(
        matches!(
            index.lookup_method_rbi(id, "totally_unknown_gap_typo_zzz", None),
            MethodLookup::NotFound
        ),
        "a genuinely unknown method on a closed, non-gem-reopening class must stay NotFound"
    );
}

// -- (2) Kernel/Object gap: instance side ----------------------------------

/// `proc`/`lambda` (Kernel, private, invisible to `CORE_INVENTORY`'s
/// public/protected harvest) and `format` (already allowlisted by
/// `core::object_method`, kept here as a guard the new fallback doesn't
/// need to touch) must all silence on a CLOSED project class — proven at
/// both API layers: `lookup_method` alone still says `NotFound` (this IS
/// the gap), `lookup_method_rbi` softens it to `Inconclusive`.
#[test]
fn closed_class_kernel_self_send_is_silenced() {
    let index = project_index_for(
        "\
class LkupGapClosedKernelCall
  sig { void }
  def known_method
  end
end
",
    );
    let id = class_id(&index, "LkupGapClosedKernelCall");
    for name in ["proc", "lambda"] {
        assert!(
            matches!(index.lookup_method(id, name), MethodLookup::NotFound),
            "`{name}` must be the pre-fix NotFound on the plain lookup (proves the gap exists)"
        );
        assert!(
            matches!(index.lookup_method_rbi(id, name, None), MethodLookup::Inconclusive),
            "`{name}` must silence via the Kernel fallback on the RBI-aware lookup"
        );
    }
}

// -- (3) Kernel/Object gap: singleton side ---------------------------------

/// Singleton counterpart: `def self.build` calling `proc {}` — the
/// receiver is the CLASS object, whose surface additionally includes
/// `Class`/`Module` (`private`, `new`, ...). Proven the same two-layer
/// way as the instance side.
#[test]
fn closed_class_singleton_kernel_self_send_is_silenced() {
    let index = project_index_for(
        "\
class LkupGapClosedSingletonKernel
  sig { void }
  def self.build
  end
end
",
    );
    let id = class_id(&index, "LkupGapClosedSingletonKernel");
    for name in ["proc", "private"] {
        assert!(
            matches!(index.lookup_singleton(id, name), MethodLookup::NotFound),
            "`{name}` must be the pre-fix NotFound on the plain singleton lookup"
        );
        assert!(
            matches!(
                index.lookup_singleton_rbi(id, name, None),
                MethodLookup::Inconclusive
            ),
            "`{name}` must silence via the Kernel/Class/Module fallback"
        );
    }
}

// -- (4) gem-reopening gap, end to end -------------------------------------

/// The headline case: a project `class ::LkupGapExt` reopening whose
/// PRIMARY definition lives in a gem. `LkupGapExt.new.gem_declared_method`
/// calls a method the gem RBI actually declares — must resolve typed and
/// silent (`rbi_method` bucket, via the EXISTING `rbi_escalate` walk now
/// reachable through `external_ancestor_starts` seeding `id`'s own path).
/// `LkupGapExt.new.totally_unknown_gap_method` calls a name NEITHER the
/// project NOR the gem RBI declares — per invariant #1 with a partial
/// view, this must ALSO stay silent (`Inconclusive`), never `NotFound`:
/// the class being a confirmed gem reopening means the project can never
/// prove the gem doesn't define it elsewhere.
#[test]
fn gem_reopening_hit_and_miss_both_stay_silent() {
    let dir = tmpdir("lkup-gap-reopen");
    let db = Db::default();
    let map = wire_rbi(
        &dir,
        "gap_ext.rbi",
        "class LkupGapExt\n  def gem_declared_method(x); end\nend\n",
    );
    RbiProject::new(&db, map.clone());
    let file = wire_project(
        &db,
        "\
class LkupGapExt
  sig { void }
  def known_method
  end
end

def use_gap_ext
  LkupGapExt.new.gem_declared_method(1)
  LkupGapExt.new.totally_unknown_gap_method
end
",
    );

    let diags = diag_codes(&db, file);
    assert!(
        diags.is_empty(),
        "a gem-reopening class must never diagnose an unknown method, hit or miss: {diags:?}"
    );

    let s: CallStats = call_stats(&db, file);
    assert!(
        s.dsl_method + s.rbi_method >= 1,
        "the RBI-declared method must resolve through rbi_escalate's existing walk \
         (dsl_method_lookup's project_ancestor_starts already seeds id's own path, \
         checked before rbi_method_lookup): {s:?}"
    );
    assert_eq!(
        s.diagnosed, 0,
        "no call site on a gem-reopening class may ever land in `diagnosed`: {s:?}"
    );

    // Same proof, pinned directly at the lookup layer (index.rs), same
    // shape as `rbi_call_resolution.rs`'s "declared_external_ancestor"
    // tests: the RBI-declared name resolves through the softened lookup,
    // the undeclared name still only reaches `Inconclusive`, never
    // `Found` (no method was fabricated) and never `NotFound`.
    let index = project_index_for(
        "\
class LkupGapExt
  sig { void }
  def known_method
  end
end
",
    );
    let id = class_id(&index, "LkupGapExt");
    assert!(
        matches!(index.lookup_method(id, "gem_declared_method"), MethodLookup::NotFound),
        "plain lookup_method must still be NotFound (proves the gap exists pre-fix)"
    );
    assert!(
        matches!(
            index.lookup_method_rbi(id, "gem_declared_method", Some(&map)),
            MethodLookup::Inconclusive
        ),
        "gem-reopening fallback must soften to Inconclusive even on a name the RBI declares \
         (the typed resolution is check.rs::rbi_escalate's job, not lookup_method_rbi's)"
    );
    assert!(
        matches!(
            index.lookup_method_rbi(id, "totally_unknown_gap_method", Some(&map)),
            MethodLookup::Inconclusive
        ),
        "gem-reopening fallback must soften to Inconclusive on a miss too — partial view, never NotFound"
    );
}

/// With no discovered `sorbet/rbi`, `gem_reopens` exits before touching
/// any map: the genuinely unknown instance method must still diagnose.
/// The class-object flip also diagnoses this fixture's bare `sig`: no
/// `extend T::Sig`, superclass, mixin, or project definition supplies it.
/// Pin both blamed names, not just a count, so neither diagnostic can
/// replace the other. The RBI-present counterpart above stays silent.
#[test]
fn project_without_rbi_keeps_reopening_diagnosing_status_quo() {
    let db = Db::default();
    let text = "\
class LkupGapExtNoRbi
  sig { void }
  def known_method
  end

  def caller_method
    totally_unknown_gap_method_no_rbi
  end
end
";
    let file = wire_project(&db, text);
    let diags = check_file(&db, file);
    let mut blamed: Vec<_> = diags
        .iter()
        .map(|d| format!("{}:{}", d.code, &text[d.start..d.end]))
        .collect();
    blamed.sort_unstable();
    assert_eq!(
        blamed,
        vec!["E0101:sig", "E0101:totally_unknown_gap_method_no_rbi"],
        "without an RBI map, both absent methods on this closed class must diagnose: {diags:?}"
    );
}
