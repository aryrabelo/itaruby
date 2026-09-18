//! Bead ita-o8l.1: method-name-keyed dynamic-mixin softening, replacing
//! bead ita-a8z's measurement-only `Global`/`BuilderName` candidates
//! (both deleted — see `index.rs`'s `MixinTargetScan`/
//! `dynamic_mixin_covers` doc comments). 194 of rails' 223 baseline
//! errors were one mechanism: `railties/lib/rails/generators/
//! app_base.rb:167-172`'s `builder_class.include(ActionMethods)`, where
//! `builder_class` is a local variable — invisible to static ancestry —
//! but `ActionMethods` is a literal constant. This softens a
//! `MethodLookup::NotFound` to `Inconclusive` only when the METHOD NAME
//! being looked up is defined by such a dynamically-mixed module (or
//! that module defines `method_missing`), never by opening any class.
//!
//! Fixture: `testdata/dynamic_mixin_method/mixin_method_softening.rb`
//! (globally-unique `Ito8l`-prefixed names — `ita check testdata/`
//! scans the whole tree as ONE project, gate c).
//!
//! MUTANTS THIS FILE MUST CATCH:
//!   1. The whole `dynamic_mixin_covers` check deleted from
//!      `soften_not_found` — `builder_dispatch_call_is_silenced` flips
//!      from empty to a real E0101 diagnostic.
//!   2. The check widened to ignore `name` (soften every `NotFound`
//!      whenever ANY dynamic-mixin target exists in the project) —
//!      `unrelated_undefined_method_still_warns` flips from 1 diagnostic
//!      to 0.
//!   3. The check re-keyed on the RECEIVING class's name (bead ita-a8z's
//!      rejected `BuilderName` candidate resurrected) —
//!      `builder_named_class_with_real_error_still_warns` flips from 1
//!      diagnostic to 0. This is the ita-a8z `BuilderName` collateral
//!      false-negative, re-measured here: that candidate silenced a
//!      real, unrelated E0101 on an innocent `*Builder`-named class
//!      three files away in its own corpus run; this fixture reproduces
//!      the exact shape (a `*Builder`-named class, present in the SAME
//!      project as the dynamic mixin, with a genuinely undefined
//!      method) and pins that it still fires.

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/dynamic_mixin_method");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap();
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, path.clone().into(), text);
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    itaruby_semantic::check_file(&db, file)
        .iter()
        .map(|d| {
            format!(
                "{}[{}]: {}",
                if d.severity == itaruby_semantic::Severity::Error { "error" } else { "warning" },
                d.code,
                d.message
            )
        })
        .collect()
}

/// SILENT: `Ito8lAppBuilder#run` self-sends `ito8l_forwarded_method`,
/// which lives ONLY on `Ito8lActionMethods` — mixed in through
/// `builder_class.include(Ito8lActionMethods)`, a receiver this checker
/// can never resolve. Ancestry looks fully closed, so without this
/// bead's fix this is a fabricated E0101 (the real 194-site rails FP
/// cluster).
#[test]
fn builder_dispatch_call_is_silenced() {
    let diags = check_fixture("mixin_method_softening.rb");
    for d in &diags {
        assert!(
            !d.contains("ito8l_forwarded_method"),
            "`ito8l_forwarded_method` is provided by a dynamically-mixed \
             module and must stay silent, got: {diags:?}"
        );
    }
}

/// FIRES (control): `Ito8lUnrelatedCaller#run` self-sends
/// `ito8l_never_provided_by_any_mixin`, a method NO dynamically-mixed
/// module in this project defines. Proves the softening is keyed on the
/// method NAME — a blanket "this project has a dynamic mixin somewhere"
/// suppressor (bead ita-a8z's rejected `Global` candidate) would go
/// silent here too.
#[test]
fn unrelated_undefined_method_still_warns() {
    let diags = check_fixture("mixin_method_softening.rb");
    let hit: Vec<_> = diags.iter().filter(|d| d.contains("ito8l_never_provided_by_any_mixin")).collect();
    assert_eq!(hit.len(), 1, "expected exactly 1 diagnostic naming the unrelated method, got: {diags:?}");
    assert!(hit[0].contains("E0101"), "expected E0101, got: {hit:?}");
}

/// FIRES (control, ita-a8z `BuilderName` collateral false-negative
/// re-measured): `Ito8lWidgetBuilder` — a class whose name ends in
/// `Builder`, in the SAME project as the dynamic mixin above — self-
/// sends `ito8l_widget_missing_method`, which nothing defines. Bead
/// ita-a8z's rejected `BuilderName` candidate opened every `*Builder`-
/// named class purely by NAME and would have silenced this; this bead's
/// fix never inspects the receiving class's name at all, so it must
/// still warn.
#[test]
fn builder_named_class_with_real_error_still_warns() {
    let diags = check_fixture("mixin_method_softening.rb");
    let hit: Vec<_> = diags.iter().filter(|d| d.contains("ito8l_widget_missing_method")).collect();
    assert_eq!(hit.len(), 1, "expected exactly 1 diagnostic naming the widget method, got: {diags:?}");
    assert!(hit[0].contains("E0101"), "expected E0101, got: {hit:?}");
}

/// Exactly 2 diagnostics total: the two controls above, and nothing
/// else — pins that the silenced call really did contribute zero, not
/// just that it doesn't mention its own method name.
#[test]
fn fixture_has_exactly_the_two_control_diagnostics() {
    let diags = check_fixture("mixin_method_softening.rb");
    assert_eq!(diags.len(), 2, "expected exactly 2 diagnostics, got: {diags:?}");
}

// ---------------------------------------------------------------------
// `method_missing`, isolated (ABANDONED — see this file's header and
// `dynamic_mixin_covers`'s doc comment). Kept OUT of `testdata/`
// deliberately: `ita check testdata/` merges the WHOLE tree into one
// project (gate c), and a `method_missing`-defining target module
// anywhere in `testdata/` would (if the clause were ever resurrected)
// soften EVERY otherwise-`NotFound` lookup in the entire tree, wiping
// out gate c's planted-E0101 assertion — exactly the measured failure
// this test pins against. An inline, single-file, throwaway project
// (same pattern as `anonymous_class_new.rs`'s `diags_of`) proves it
// with zero blast radius on the real fixture tree.
// ---------------------------------------------------------------------

fn diags_of(text: &str) -> Vec<String> {
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, "inline/ito8l_method_missing.rb".into(), text.to_string());
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    itaruby_semantic::check_file(&db, file)
        .iter()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect()
}

/// Regression pin for the ABANDON decision: `Ito8lMmForwarder` defines
/// only `method_missing` (no `ito8l_mm_anything` method literally
/// named) and is mixed in through a dynamic receiver — the exact shape
/// the task's third clause targeted. The self-send of an unrelated
/// undefined name on the includer must still warn: `dynamic_mixin_
/// covers` only checks each target's OWN `methods` map for the literal
/// `name`, and never special-cases `method_missing`.
#[test]
fn method_missing_softening_does_not_cause_blanket_silencing() {
    let diags = diags_of(
        r"
module Ito8lMmForwarder
  def method_missing(name, *args)
    super
  end
end

module Ito8lMmSetup
  def self.wire(target)
    target.include(Ito8lMmForwarder)
  end
end

class Ito8lMmHost
  def run
    ito8l_mm_whatever_name
  end
end
",
    );
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0101"), "expected E0101, got: {diags:?}");
    assert!(
        diags[0].contains("ito8l_mm_whatever_name"),
        "message should name the missing method, got: {diags:?}"
    );
}

/// Guard rail: with NO dynamic mixin machinery in the project at all,
/// the exact same undefined self-send still warns — a plain control
/// with zero mixin machinery, distinct from the previous test (mixin
/// machinery present, but only `method_missing` — no literal `name` —
/// is defined by it).
#[test]
fn same_undefined_call_warns_without_any_dynamic_mixin() {
    let diags = diags_of(
        r"
class Ito8lMmHostAlone
  def run
    ito8l_mm_whatever_name
  end
end
",
    );
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0101"), "expected E0101, got: {diags:?}");
}
