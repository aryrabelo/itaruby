//! Integration follow-up to bead ita-p24: once malformed sig comments
//! stopped dying as E0105, genuine SUBTYPE calls fired E0103, because
//! `compatible()` did exact `Ty::Instance` equality with no ancestry
//! awareness — e.g. `add(entry: Entry)` called with an `Entry::Method`
//! (measured on ruby-lsp/tapioca: +10 new false E0103, all genuine
//! subtypes, verified by hand).
//!
//! Since MATRIX H2 the `#:` RBS path judges only arguments WRITTEN as
//! literals, like a Sorbet `sig`: an instance (`X.new`) is never judged,
//! whatever its ancestry, so every instance call below is silent and the
//! ancestry arms of `compatible()` were removed as unreachable. What an
//! RBS sig still accuses is a literal against a class param; a literal
//! against a MODULE param stays silent (reflection can mix a module into
//! any core class).

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

/// A subclass instance IS the param class at runtime, and is not a
/// literal either: no E0103.
#[test]
fn subclass_instance_arg_is_compatible() {
    let src = "class CompatEntry\nend\n\nclass CompatEntryMethod < CompatEntry\nend\n\nclass CompatService\n  #: (CompatEntry) -> void\n  def add(entry)\n  end\nend\n\nCompatService.new.add(CompatEntryMethod.new)\n";
    assert!(
        !diags_of(src).iter().any(|d| d.starts_with("E0103")),
        "subclass arg must not fire E0103: {:?}",
        diags_of(src)
    );
}

/// An unrelated class instance is not a literal: never judged, even with
/// a fully-resolved ancestry (MATRIX H2). A literal against the same
/// class param is the one E0103, at the literal.
#[test]
fn unrelated_instance_is_silent_and_a_literal_accuses() {
    let src = "class CompatEntryB\nend\n\nclass CompatUnrelated\nend\n\nclass CompatServiceB\n  #: (CompatEntryB) -> void\n  def add(entry)\n  end\nend\n\nCompatServiceB.new.add(CompatUnrelated.new)\nCompatServiceB.new.add(\"entry\")\n";
    assert_eq!(
        diags_of(src),
        ["E0103 argument 1 of `add` expects CompatEntryB, got String"],
        "only the literal is judged"
    );
}

/// Arg class whose superclass never resolves: an instance, never judged
/// (MATRIX H2), so the call stays silent even with an unrelated param.
#[test]
fn open_ancestry_stays_silent() {
    let src = "class CompatMysteryChild < CompatGhostParent\nend\n\nclass CompatEntryC\nend\n\nclass CompatServiceC\n  #: (CompatEntryC) -> void\n  def add(entry)\n  end\nend\n\nCompatServiceC.new.add(CompatMysteryChild.new)\n";
    assert!(
        !diags_of(src).iter().any(|d| d.starts_with("E0103")),
        "unprovable non-subtype must stay silent: {:?}",
        diags_of(src)
    );
}

/// A module as the param type: never proof against anything, so neither
/// an includer's instance nor a literal accuses.
#[test]
fn module_param_is_compatible() {
    let src = "module CompatWalkable\nend\n\nclass CompatWalker\n  include CompatWalkable\nend\n\nclass CompatServiceD\n  #: (CompatWalkable) -> void\n  def guide(who)\n  end\nend\n\nCompatServiceD.new.guide(CompatWalker.new)\nCompatServiceD.new.guide(\"walker\")\n";
    assert!(
        !diags_of(src).iter().any(|d| d.starts_with("E0103")),
        "module param must not fire E0103: {:?}",
        diags_of(src)
    );
}
