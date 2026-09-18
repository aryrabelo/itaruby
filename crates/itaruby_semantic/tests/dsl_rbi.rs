//! `index.rs::dsl_method_lookup` (bead ita-tjr, on top of ita-xze/ita-uh1's
//! `rbi_method_lookup`): does the client's Tapioca DSL RBI for a project
//! class ITSELF (or one of its PROJECT ancestors) declare a method — the
//! `sorbet/rbi/dsl/` shape, which reopens the app's OWN models, never an
//! external gem. Aditive-only, same contract as `rbi_method_lookup`: a
//! `None` changes nothing observable, and this file never asserts on
//! `open`/`OpenReason` changing (it can't — nothing here touches either).
//!
//! The load-bearing shape under test is real, not synthetic flavor: a
//! Tapioca DSL RBI writes `class Foo; include GeneratedAssociationMethods;
//! ...; end` with the module itself nested UNQUALIFIED later in the SAME
//! file (confirmed against a real corpus's `dsl/package.rbi`, lead review,
//! bead ita-tjr). Every fixture below mirrors that exact nesting, never the
//! flat single-class shape `rbi_methods.rs`'s gem fixtures use — a flat
//! fixture would never exercise the nested-edge resolution this bead
//! exists for.
//!
//! Fixtures are toy `.rbi` files written to a per-test tempdir
//! (`std::env::temp_dir()`, mirroring `rbi_methods.rs`), never `testdata/`
//! — a corpus-scan root would leak these synthetic classes across every
//! other fixture file. Every test uses a DISTINCT top-level class/module
//! name: `rbi_method_closure`'s memo is keyed process-wide on the start
//! name alone, so two tests sharing a name would serve each other's cached
//! method set and fail by run order, not by behavior (this already broke a
//! test once this session — see `rbi_methods.rs`'s header comment).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("itaruby-dsl-rbi-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Writes one `.rbi` file under `<dir>/sorbet/rbi/dsl/<filename>` and
/// returns the `constant -> file` map `discover_rbi_files`/`build_rbi_index`
/// produce for the WHOLE `sorbet/rbi/dsl` tree so far — the exact `rbi_map`
/// shape `dsl_method_lookup` takes. Callers that need two DSL files present
/// at once (the file-precedence test) call this twice, under two different
/// `filename`s; the second call's returned map covers both.
fn write_dsl_rbi(dir: &Path, filename: &str, content: &str) -> HashMap<String, Vec<PathBuf>> {
    let rbi_dir = dir.join("sorbet/rbi/dsl");
    std::fs::create_dir_all(&rbi_dir).unwrap();
    std::fs::write(rbi_dir.join(filename), content).unwrap();
    let files = itaruby_semantic::rbi::discover_rbi_files(&dir.join("sorbet/rbi"));
    itaruby_semantic::rbi::build_rbi_index(&files).constants
}

/// One project file's `ProjectIndex`, independent of the checker plumbing
/// (mirrors `rbi_methods.rs::project_index_for`).
fn project_index_for(dir: &Path, text: &str) -> itaruby_semantic::index::ProjectIndex {
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, dir.join("project.rb"), text.to_string());
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

/// The headline shape: `class DslNestOnlyWidget; include
/// DslNestOnlyGenAttrs; end` with `module DslNestOnlyWidget::
/// DslNestOnlyGenAttrs` nested later in the SAME file, `include`d
/// unqualified — exactly how Tapioca renders a model's generated
/// attribute accessors. The queried method exists ONLY inside that
/// nested, unqualified module; it must resolve, carrying the sig's own
/// type.
///
/// Without nested-edge resolution ((b) in `resolve_method_node`'s doc
/// comment) this test fails: the raw edge text `DslNestOnlyGenAttrs`
/// does not match any TOP-LEVEL header in this file (there isn't one —
/// the module is nested), so a global-map-only resolver would need the
/// name to exist somewhere in the process-wide `rbi_map` under that
/// exact literal spelling, which nothing here declares. Confirmed by
/// temporarily removing the `format!("{{parent_path}}::{{node}}")`
/// branch and rerunning this test alone: it fails (`None`) with that
/// branch gone, and passes with it restored — this assertion is the
/// proof the nested-resolution rule is actually load-bearing here, not
/// incidentally passing some other way.
#[test]
fn method_only_in_unqualified_nested_module_resolves() {
    let dir = tempdir("nest-only");
    let map = write_dsl_rbi(
        &dir,
        "dsl_nest_only_widget.rbi",
        "class DslNestOnlyWidget\n  include DslNestOnlyGenAttrs\n\n  module DslNestOnlyGenAttrs\n    sig { returns(String) }\n    def nest_only_field; end\n  end\nend\n",
    );
    let proj = project_index_for(&dir, "class DslNestOnlyWidget\nend\n");
    let id = class_id(&proj, "DslNestOnlyWidget");

    let ty = itaruby_semantic::index::dsl_method_lookup(&proj, id, "nest_only_field", false, &map);
    assert_eq!(
        ty,
        Some(itaruby_semantic::types::Ty::Str),
        "method declared only in the unqualified nested module must resolve with the sig's own type"
    );
}

/// File precedence (the collision guard, bead ita-tjr's core claim):
/// TWO different DSL files, each nesting a module of the SAME generated
/// name (`DslPrecGenAttrs`) inside its own class, each declaring a
/// DIFFERENT method. Phase 1's line scan (`rbi.rs::build_rbi_index`)
/// necessarily maps the bare name `DslPrecGenAttrs` to only ONE of the
/// two files (first occurrence wins) — if `dsl_method_lookup` ever fell
/// back to that global map for this name, one of the two project
/// classes below would silently receive the OTHER class's accessors
/// (and lose its own). Every assertion here has to hold simultaneously
/// for the fix to be proven: each class sees exactly its own file's
/// method and never the other's.
#[test]
fn nested_module_precedence_stays_per_file() {
    let dir = tempdir("precedence");
    write_dsl_rbi(
        &dir,
        "dsl_prec_invoice.rbi",
        "class DslPrecInvoice\n  include DslPrecGenAttrs\n\n  module DslPrecGenAttrs\n    sig { returns(String) }\n    def invoice_only_field; end\n  end\nend\n",
    );
    let map = write_dsl_rbi(
        &dir,
        "dsl_prec_expense.rbi",
        "class DslPrecExpense\n  include DslPrecGenAttrs\n\n  module DslPrecGenAttrs\n    sig { returns(Integer) }\n    def expense_only_field; end\n  end\nend\n",
    );
    let proj = project_index_for(
        &dir,
        "class DslPrecInvoice\nend\nclass DslPrecExpense\nend\n",
    );
    let invoice_id = class_id(&proj, "DslPrecInvoice");
    let expense_id = class_id(&proj, "DslPrecExpense");

    assert_eq!(
        itaruby_semantic::index::dsl_method_lookup(&proj, invoice_id, "invoice_only_field", false, &map),
        Some(itaruby_semantic::types::Ty::Str),
        "DslPrecInvoice must see its OWN nested module's method"
    );
    assert_eq!(
        itaruby_semantic::index::dsl_method_lookup(&proj, invoice_id, "expense_only_field", false, &map),
        None,
        "DslPrecInvoice must NEVER see DslPrecExpense's method through the colliding global name"
    );
    assert_eq!(
        itaruby_semantic::index::dsl_method_lookup(&proj, expense_id, "expense_only_field", false, &map),
        Some(itaruby_semantic::types::Ty::Int),
        "DslPrecExpense must see its OWN nested module's method"
    );
    assert_eq!(
        itaruby_semantic::index::dsl_method_lookup(&proj, expense_id, "invoice_only_field", false, &map),
        None,
        "DslPrecExpense must NEVER see DslPrecInvoice's method through the colliding global name"
    );
}

/// Miss: the DSL RBI is real and wired, but never declares the queried
/// name anywhere in `DslMissWidget`'s own file. `dsl_method_lookup`
/// changes nothing observable on a miss (aditive-only) — no diagnostic
/// is possible here because the function is a pure query, never a
/// diagnostic emitter.
#[test]
fn nonexistent_method_stays_a_miss_with_no_diagnostic() {
    let dir = tempdir("dsl-miss");
    let map = write_dsl_rbi(
        &dir,
        "dsl_miss_widget.rbi",
        "class DslMissWidget\n  include DslMissGenAttrs\n\n  module DslMissGenAttrs\n    sig { returns(String) }\n    def real_field; end\n  end\nend\n",
    );
    let proj = project_index_for(&dir, "class DslMissWidget\nend\n");
    let id = class_id(&proj, "DslMissWidget");

    assert_eq!(
        itaruby_semantic::index::dsl_method_lookup(&proj, id, "field_that_does_not_exist", false, &map),
        None,
        "a name the DSL RBI never declares must stay a miss"
    );
}

/// Rule 4 (`class << self` / `def self.x` lands in the SINGLETON map
/// unconditionally): a method declared inside `class << self` on the
/// project class's own DSL fragment must answer a singleton query and
/// must NOT answer an instance query.
#[test]
fn singleton_class_body_method_lands_in_the_singleton_set() {
    let dir = tempdir("dsl-singleton");
    let map = write_dsl_rbi(
        &dir,
        "dsl_singleton_widget.rbi",
        "class DslSingletonWidget\n  class << self\n    sig { returns(String) }\n    def singleton_only_field; end\n  end\nend\n",
    );
    let proj = project_index_for(&dir, "class DslSingletonWidget\nend\n");
    let id = class_id(&proj, "DslSingletonWidget");

    assert_eq!(
        itaruby_semantic::index::dsl_method_lookup(&proj, id, "singleton_only_field", true, &map),
        Some(itaruby_semantic::types::Ty::Str),
        "a `class << self` method must answer a singleton query"
    );
    assert_eq!(
        itaruby_semantic::index::dsl_method_lookup(&proj, id, "singleton_only_field", false, &map),
        None,
        "a `class << self` method must NOT answer an instance query"
    );
}

/// Zero-cost miss: a project class with no DSL RBI declared for it at
/// all (the `rbi_map` here only ever mentions an UNRELATED class) — no
/// start name resolves through `rbi_map`, so the walk immediately falls
/// through to a miss without ever opening a file for this class.
#[test]
fn project_class_without_any_dsl_rbi_is_a_zero_cost_miss() {
    let dir = tempdir("dsl-absent");
    let map = write_dsl_rbi(
        &dir,
        "dsl_absent_unrelated.rbi",
        "class DslAbsentUnrelated\n  sig { returns(String) }\n  def unrelated_field; end\nend\n",
    );
    let proj = project_index_for(&dir, "class DslAbsentNoRbi\nend\n");
    let id = class_id(&proj, "DslAbsentNoRbi");

    assert_eq!(
        itaruby_semantic::index::dsl_method_lookup(&proj, id, "unrelated_field", false, &map),
        None,
        "a project class with no DSL RBI at all must be a plain miss"
    );
}

/// Aditive-only lock (batch contract): a class whose methods now resolve
/// through its own DSL RBI keeps the EXACT SAME `open`/`OpenReason` it
/// had before any of this bead's code ran — `dsl_method_lookup` takes
/// `&ProjectIndex`, never `&mut`, so it structurally cannot flip either
/// field; this test asserts the concrete values explicitly rather than
/// relying on the type system alone to make the claim legible.
#[test]
fn dsl_hit_never_touches_open_or_open_reason() {
    let dir = tempdir("dsl-open-untouched");
    let map = write_dsl_rbi(
        &dir,
        "dsl_open_guard_widget.rbi",
        "class DslOpenGuardWidget\n  include DslOpenGuardGenAttrs\n\n  module DslOpenGuardGenAttrs\n    sig { returns(String) }\n    def guarded_field; end\n  end\nend\n",
    );
    // `method_missing` is a real, independent open reason (bead ita-anc) —
    // unrelated to anything this bead touches — so the class starts open
    // for a KNOWN reason this test can assert on before and after.
    let proj = project_index_for(
        &dir,
        "class DslOpenGuardWidget\n  def method_missing(m, *a)\n  end\nend\n",
    );
    let id = class_id(&proj, "DslOpenGuardWidget");

    assert!(proj.class(id).open, "fixture must start open");
    assert_eq!(
        proj.class(id).open_reason,
        Some(itaruby_semantic::index::OpenReason::MethodMissing),
        "fixture's open reason must be the known method_missing hook"
    );

    assert_eq!(
        itaruby_semantic::index::dsl_method_lookup(&proj, id, "guarded_field", false, &map),
        Some(itaruby_semantic::types::Ty::Str),
        "the DSL RBI hit itself must still resolve"
    );

    assert!(
        proj.class(id).open,
        "open must be untouched by a DSL RBI hit (aditive-only)"
    );
    assert_eq!(
        proj.class(id).open_reason,
        Some(itaruby_semantic::index::OpenReason::MethodMissing),
        "open_reason must be untouched by a DSL RBI hit (aditive-only)"
    );
}
