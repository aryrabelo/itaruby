//! Integration follow-up to bead ita-p24: once malformed sig comments
//! stopped dying as E0105, genuine SUBTYPE calls fired E0103, because
//! `compatible()` did exact `Ty::Instance` equality with no ancestry
//! awareness — e.g. `add(entry: Entry)` called with an `Entry::Method`
//! (measured on ruby-lsp/tapioca: +10 new false E0103, all genuine
//! subtypes, verified by hand). The fix is ancestry-aware and fail-closed:
//! a param class in the arg's ancestry is compatible; an INCOMPLETE
//! ancestry can never prove non-subtyping, so it stays silent
//! (invariant #1).
//!
//! MUTANTS THIS FILE MUST CATCH:
//!   1. The `Ty::Instance(a), Ty::Instance(p)` arm reduced back to
//!      `a == p` — `subclass_instance_arg_is_compatible` and
//!      `module_param_is_compatible` flip to an E0103 diagnostic.
//!   2. The arm broadened to unconditional `true` —
//!      `unrelated_class_still_accuses` flips to silence.
//!   3. The fail-closed guard inverted (`complete` instead of
//!      `!complete`) — `open_ancestry_stays_silent` flips to E0103.

use itaruby_semantic::{check_file, Db, ProjectFiles, SourceFile};

fn diags_of(text: &str) -> Vec<String> {
    let db = Db::default();
    let file = SourceFile::new(&db, "inline/compat_subclass.rb".into(), text.to_string());
    ProjectFiles::new(&db, vec![file]);
    check_file(&db, file)
        .iter()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect()
}

/// A subclass instance IS the param class at runtime: no E0103.
#[test]
fn subclass_instance_arg_is_compatible() {
    let src = "class CompatEntry\nend\n\nclass CompatEntryMethod < CompatEntry\nend\n\nclass CompatService\n  #: (CompatEntry) -> void\n  def add(entry)\n  end\nend\n\nCompatService.new.add(CompatEntryMethod.new)\n";
    assert!(
        !diags_of(src).iter().any(|d| d.starts_with("E0103")),
        "subclass arg must not fire E0103: {:?}",
        diags_of(src)
    );
}

/// An unrelated class with a fully-resolved ancestry still fires E0103 —
/// the subclass arm must not become blanket silence.
#[test]
fn unrelated_class_still_accuses() {
    let src = "class CompatEntryB\nend\n\nclass CompatUnrelated\nend\n\nclass CompatServiceB\n  #: (CompatEntryB) -> void\n  def add(entry)\n  end\nend\n\nCompatServiceB.new.add(CompatUnrelated.new)\n";
    let diags = diags_of(src);
    assert!(
        diags.iter().any(|d| d.starts_with("E0103")),
        "unrelated arg must keep firing E0103, got: {diags:?}"
    );
}

/// Arg class whose superclass never resolves: non-subtyping is
/// unprovable, so the call stays silent even with an unrelated param.
#[test]
fn open_ancestry_stays_silent() {
    let src = "class CompatMysteryChild < CompatGhostParent\nend\n\nclass CompatEntryC\nend\n\nclass CompatServiceC\n  #: (CompatEntryC) -> void\n  def add(entry)\n  end\nend\n\nCompatServiceC.new.add(CompatMysteryChild.new)\n";
    assert!(
        !diags_of(src).iter().any(|d| d.starts_with("E0103")),
        "unprovable non-subtype must stay silent: {:?}",
        diags_of(src)
    );
}

/// A module as the param type: `include` puts it in the arg class's
/// ancestry, so the call is compatible.
#[test]
fn module_param_is_compatible() {
    let src = "module CompatWalkable\nend\n\nclass CompatWalker\n  include CompatWalkable\nend\n\nclass CompatServiceD\n  #: (CompatWalkable) -> void\n  def guide(who)\n  end\nend\n\nCompatServiceD.new.guide(CompatWalker.new)\n";
    assert!(
        !diags_of(src).iter().any(|d| d.starts_with("E0103")),
        "included-module param must not fire E0103: {:?}",
        diags_of(src)
    );
}
