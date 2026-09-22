//! Regression controls from the independent ancestry review.
//! Each public repro checks both file orders and consumer-visible behavior.
use itaruby_semantic::index::MethodLookup;
use itaruby_semantic::{Db, ProjectFiles, SourceFile};

fn project(sources: &[(&str, &str)], reverse: bool) -> (Db, Vec<SourceFile>) {
    let db = Db::default();
    let files: Vec<_> = sources.iter().map(|(path, text)| {
        SourceFile::new(&db, format!("/ancestry_review/{path}").into(), (*text).to_string())
    }).collect();
    let mut order = files.clone();
    if reverse {
        order.reverse();
    }
    ProjectFiles::new(&db, order);
    (db, files)
}

#[test]
fn intermediate_conflicting_namespace_keeps_relative_and_cbase_paths_unknown() {
    let sources = [
        ("bases.rb", "class AlphaBase\n  module Subscriber\n    VALUE = 1\n  end\nend\nclass BetaBase; end"),
        ("a.rb", "class SharedSuite < AlphaBase; end"),
        ("b.rb", "class SharedSuite < BetaBase; end"),
        ("calls.rb", "SharedSuite::Subscriber::VALUE\n::SharedSuite::Subscriber::VALUE"),
    ];
    for reverse in [false, true] {
        let (db, files) = project(&sources, reverse);
        let diagnostics = itaruby_semantic::check_file(&db, files[3]);
        assert!(diagnostics.is_empty(), "intermediate ambiguity must not emit E0104: {diagnostics:?}");
        let index = itaruby_semantic::project_index(&db);
        for path in ["SharedSuite::Subscriber::VALUE", "::SharedSuite::Subscriber::VALUE"] {
            assert!(index.const_exists(&[], path));
            assert!(index.resolve_const_through_aliases(&[], path).is_none(), "uncertainty is not a proven type");
        }
    }
}

#[test]
fn schema_cannot_invent_attributes_from_an_enclosing_conflict() {
    let sources = [
        ("bases.rb", "class AlphaBase\n  class ApplicationRecord; end\nend\nclass BetaBase; end\nclass ApplicationRecord; end"),
        ("a.rb", "class SharedSuite < AlphaBase; end"),
        ("b.rb", "class SharedSuite < BetaBase; end"),
        ("nested.rb", "class SharedSuite\n  class Widget < ApplicationRecord; end\nend"),
        ("db/schema.rb", "ActiveRecord::Schema.define do\n  create_table 'widgets' do |t|\n    t.string 'title'\n  end\nend"),
        ("calls.rb", "SharedSuite::Widget.new.title(1)"),
    ];
    for reverse in [false, true] {
        let (db, files) = project(&sources, reverse);
        let diagnostics = itaruby_semantic::check_file(&db, files[5]);
        assert!(diagnostics.is_empty(), "an unresolved header cannot supply schema arity: {diagnostics:?}");
        let index = itaruby_semantic::project_index(&db);
        assert!(matches!(index.lookup_method(index.by_path["SharedSuite::Widget"], "title"), MethodLookup::Inconclusive));
        let offset = sources[5].1.find("title").unwrap();
        assert!(itaruby_semantic::definition_at(&db, files[5], offset).is_none(), "no navigation to a fabricated schema attribute");
    }
}

#[test]
fn class_reopening_is_not_a_fallback_past_conflicting_singleton_ancestry() {
    let sources = [
        ("bases.rb", "class Class\n  def ancestry_probe(value); value; end\nend\nclass AlphaBase\n  def self.ancestry_probe(*values); end\nend\nclass BetaBase\n  def self.ancestry_probe(*values); end\nend"),
        ("a.rb", "class SharedSuite < AlphaBase; end"),
        ("b.rb", "class SharedSuite < BetaBase; end"),
        ("calls.rb", "SharedSuite.ancestry_probe(1, 2)"),
    ];
    for reverse in [false, true] {
        let (db, files) = project(&sources, reverse);
        let diagnostics = itaruby_semantic::check_file(&db, files[3]);
        assert!(diagnostics.is_empty(), "Class fallback is shadowed by unknown ancestry: {diagnostics:?}");
        let index = itaruby_semantic::project_index(&db);
        assert!(matches!(index.lookup_singleton(index.by_path["SharedSuite"], "ancestry_probe"), MethodLookup::Inconclusive));
        let offset = sources[3].1.find("ancestry_probe").unwrap();
        assert!(itaruby_semantic::definition_at(&db, files[3], offset).is_none(), "Class#ancestry_probe is not a proven dispatch target");
    }
}

#[test]
fn directly_defined_new_keeps_its_arity_and_navigation_under_base_conflict() {
    let sources = [
        ("bases.rb", "class AlphaBase; end\nclass BetaBase; end"),
        ("a.rb", "class SharedSuite < AlphaBase\n  def self.new(value); value; end\nend"),
        ("b.rb", "class SharedSuite < BetaBase; end"),
        ("calls.rb", "SharedSuite.new(1, 2)"),
    ];
    for reverse in [false, true] {
        let (db, files) = project(&sources, reverse);
        let diagnostics = itaruby_semantic::check_file(&db, files[3]);
        assert_eq!(diagnostics.len(), 1, "own constructor arity remains provable: {diagnostics:?}");
        assert_eq!(diagnostics[0].code, "E0102");
        let offset = sources[3].1.find("new").unwrap();
        let definition = itaruby_semantic::definition_at(&db, files[3], offset).expect("own constructor must navigate");
        assert_eq!(definition.file, files[1]);
        let index = itaruby_semantic::project_index(&db);
        match index.lookup_singleton_own(index.by_path["SharedSuite"], "new") {
            MethodLookup::Found(method, owner) => {
                assert_eq!(owner, index.by_path["SharedSuite"]);
                assert_eq!(method.required, 1);
            }
            other => panic!("own constructor must stay known: {other:?}"),
        }
    }
}

#[test]
fn each_alias_hop_respects_its_write_scope_before_global_fallback() {
    let sources = [
        ("bases.rb", "class RightTarget\n  def probe(*values); end\nend\nclass WrongTarget\n  def probe(value); end\nend\nGlobalAlias = WrongTarget\nclass AlphaBase\n  GlobalAlias = ::RightTarget\nend\nclass BetaBase\n  GlobalAlias = ::RightTarget\nend"),
        ("a.rb", "class SharedSuite < AlphaBase\n  LocalAlias = GlobalAlias\nend"),
        ("b.rb", "class SharedSuite < BetaBase; end"),
        ("calls.rb", "SharedSuite::LocalAlias.new.probe(1, 2)"),
    ];
    for reverse in [false, true] {
        let (db, files) = project(&sources, reverse);
        let diagnostics = itaruby_semantic::check_file(&db, files[3]);
        assert!(diagnostics.is_empty(), "an alias hop cannot pick the global WrongTarget: {diagnostics:?}");
        let offset = sources[3].1.find("probe").unwrap();
        assert!(itaruby_semantic::definition_at(&db, files[3], offset).is_none(), "an inherited alias is not proof of WrongTarget#probe");
        let index = itaruby_semantic::project_index(&db);
        assert!(index.resolve_const_through_aliases(&[], "SharedSuite::LocalAlias").is_none());
        assert!(index.const_exists(&[], "SharedSuite::LocalAlias"));
    }
}

#[test]
fn aliases_retain_uncertainty_from_intermediate_qualified_namespaces() {
    let sources = [
        ("bases.rb", "class AlphaBase\n  module Subscriber\n    VALUE = 1\n  end\nend\nclass BetaBase; end"),
        ("a.rb", "class SharedSuite < AlphaBase; end"),
        ("b.rb", "class SharedSuite < BetaBase; end"),
        ("calls.rb", "Chosen = SharedSuite::Subscriber\nChosen::VALUE\n::Chosen::VALUE"),
    ];
    for reverse in [false, true] {
        let (db, files) = project(&sources, reverse);
        let diagnostics = itaruby_semantic::check_file(&db, files[3]);
        assert!(diagnostics.is_empty(), "alias RHS uncertainty must survive relative and cbase uses: {diagnostics:?}");
        let index = itaruby_semantic::project_index(&db);
        for path in ["Chosen::VALUE", "::Chosen::VALUE"] {
            assert!(index.const_exists(&[], path));
            assert!(index.resolve_const_through_aliases(&[], path).is_none());
        }
    }
}

#[test]
fn proven_lexical_alias_keeps_arity_and_navigation_under_conflict() {
    let sources = [
        ("bases.rb", "class Target\n  def probe(value); end\nend\nclass AlphaBase; end\nclass BetaBase; end"),
        ("a.rb", "class SharedSuite < AlphaBase\n  Known = ::Target\n  def exercise\n    Known.new.probe(1, 2)\n  end\nend"),
        ("b.rb", "class SharedSuite < BetaBase; end"),
    ];
    for reverse in [false, true] {
        let (db, files) = project(&sources, reverse);
        let diagnostics = itaruby_semantic::check_file(&db, files[1]);
        assert_eq!(diagnostics.len(), 1, "a proven lexical alias must retain its arity check: {diagnostics:?}");
        assert_eq!(diagnostics[0].code, "E0102");
        let offset = sources[1].1.find("probe").unwrap();
        let definition = itaruby_semantic::definition_at(&db, files[1], offset).expect("proven lexical alias must navigate");
        assert_eq!(definition.file, files[0]);
        let index = itaruby_semantic::project_index(&db);
        let target = Some(index.by_path["Target"]);
        assert_eq!(index.resolve_const_through_aliases(&["SharedSuite".into()], "Known"), target);
        assert_eq!(index.resolve_const_through_aliases(&[], "::SharedSuite::Known"), target);
    }
}

#[test]
fn alias_cycles_are_inconclusive_without_hiding_known_values_or_classes() {
    let sources = [
        ("targets.rb", "class Target\n  VALUE = 1\n  def probe(value); end\nend\nKnown = ::Target"),
        ("cycle.rb", "CycleA = CycleB\nCycleB = CycleA"),
        ("calls.rb", "CycleA::VALUE\n::CycleB::VALUE\nKnown::VALUE\nKnown.new.probe(1, 2)"),
    ];
    for reverse in [false, true] {
        let (db, files) = project(&sources, reverse);
        let index = itaruby_semantic::project_index(&db);
        assert!(index.resolve_const_through_aliases(&[], "CycleA").is_none());
        assert!(index.const_exists(&[], "CycleA::VALUE"));
        assert!(index.const_exists(&[], "::Target::VALUE"));
        assert!(index.resolve_const_through_aliases(&[], "::Target::VALUE").is_none());
        assert!(!index.const_exists(&[], "::Target::MISSING_VALUE"));
        assert_eq!(index.resolve_const_through_aliases(&[], "Known"), Some(index.by_path["Target"]));
        let diagnostics = itaruby_semantic::check_file(&db, files[2]);
        assert_eq!(diagnostics.len(), 1, "only the known method arity is conclusive: {diagnostics:?}");
        assert_eq!(diagnostics[0].code, "E0102");
    }
}

#[test]
fn alias_hop_limit_is_inconclusive_and_does_not_leak_between_queries() {
    use std::fmt::Write;
    let mut aliases = String::new();
    // Forty hops exercise the 32-hop guard without needing dangerous depth.
    for i in 0..40 {
        writeln!(aliases, "A{i} = A{}", i + 1).unwrap();
    }
    aliases.push_str("A40 = ::Target\n");
    let sources = [
        ("targets.rb", "class Target\n  VALUE = 1\n  def probe(value); end\nend"),
        ("aliases.rb", aliases.as_str()),
        ("calls.rb", "A0::VALUE\n::A0::VALUE\nA40::VALUE\nA40.new.probe(1, 2)"),
    ];
    for reverse in [false, true] {
        let (db, files) = project(&sources, reverse);
        let index = itaruby_semantic::project_index(&db);
        assert!(index.resolve_const_through_aliases(&[], "A0").is_none(), "a path beyond the budget must not invent a type");
        assert!(index.const_exists(&[], "A0::VALUE"));
        assert_eq!(index.resolve_const_through_aliases(&[], "A40"), Some(index.by_path["Target"]));
        let diagnostics = itaruby_semantic::check_file(&db, files[2]);
        assert_eq!(diagnostics.len(), 1, "only the immediate alias's arity remains conclusive: {diagnostics:?}");
        assert_eq!(diagnostics[0].code, "E0102");
    }
}

#[test]
fn alias_budget_is_shared_across_qualified_segments() {
    use std::fmt::Write;
    let depth = 36;
    let mut declarations = String::new();
    let mut direct = String::from("C0");
    let mut aliased = String::from("Entry");
    for i in 0..=depth {
        writeln!(declarations, "class C{i}").unwrap();
        if i > 0 {
            write!(direct, "::C{i}").unwrap();
            aliased.push_str("::Hop");
        }
    }
    declarations.push_str("VALUE = 1\ndef self.probe(value); end\n");
    for i in (0..depth).rev() {
        writeln!(declarations, "end\nHop = C{}", i + 1).unwrap();
    }
    declarations.push_str("end\nEntry = C0\n");
    let calls = format!("{aliased}::VALUE\n::{direct}::VALUE\n::{direct}.probe(1, 2)");
    let sources = [("declarations.rb", declarations.as_str()), ("calls.rb", calls.as_str())];
    for reverse in [false, true] {
        let (db, files) = project(&sources, reverse);
        let index = itaruby_semantic::project_index(&db);
        assert!(index.resolve_const_through_aliases(&[], &aliased).is_none(), "separate segments cannot each reset the alias budget");
        assert!(index.const_exists(&[], &format!("{aliased}::VALUE")));
        assert_eq!(index.resolve_const_through_aliases(&[], "Entry"), Some(index.by_path["C0"]));
        assert_eq!(index.resolve_const_through_aliases(&[], &direct), Some(index.by_path[&direct]));
        let diagnostics = itaruby_semantic::check_file(&db, files[1]);
        assert_eq!(diagnostics.len(), 1, "the direct class still provides a known arity: {diagnostics:?}");
        assert_eq!(diagnostics[0].code, "E0102");
    }
}

#[test]
fn a_dynamic_lexical_value_shadows_an_outer_alias_instead_of_resurrecting_it() {
    // The inner `Known` is a runtime value, unknowable here; it shadows the
    // outer/global literal alias, so the reference must stay Unknown rather
    // than pick `WrongTarget`. A control class with NO inner shadow still
    // resolves the same outer alias, proving the walk is not globally broken.
    let bases = "class RuntimeTarget\n  def probe(*values); end\nend\nclass WrongTarget\n  def probe(value); end\nend\nKnown = ::WrongTarget";
    let plain = [
        ("bases.rb", bases),
        ("scope.rb", "class Scope\n  Known = Object.const_get(\"RuntimeTarget\")\n  def exercise\n    Known.new.probe(1, 2)\n  end\nend\nclass Plain\n  def exercise\n    Known.new.probe(1, 2)\n  end\nend"),
    ];
    let conflict = [
        ("bases.rb", bases),
        ("a.rb", "class AlphaBase; end\nclass BetaBase; end\nmodule Outer\n  Known = ::WrongTarget\n  class SharedSuite < ::AlphaBase\n    Known = Object.const_get(\"RuntimeTarget\")\n    def exercise\n      Known.new.probe(1, 2)\n    end\n  end\nend"),
        ("b.rb", "class Outer::SharedSuite < ::BetaBase; end"),
    ];
    for reverse in [false, true] {
        let (db, files) = project(&plain, reverse);
        let index = itaruby_semantic::project_index(&db);
        // Silent side: the shadowed dynamic value is Unknown, no wrong arity.
        assert!(index.resolve_const_through_aliases(&["Scope".into()], "Known").is_none());
        let scope = files.iter().copied().find(|f| index_path_ends(&db, *f, "scope.rb")).unwrap();
        let scope_diags = itaruby_semantic::check_file(&db, scope);
        assert_eq!(scope_diags.len(), 1, "only the unshadowed control accuses: {scope_diags:?}");
        assert_eq!(scope_diags[0].code, "E0102");
        // Accusing side: with no inner shadow the SAME outer alias resolves.
        assert_eq!(index.resolve_const_through_aliases(&["Plain".into()], "Known"), Some(index.by_path["WrongTarget"]));

        let (cdb, cfiles) = project(&conflict, reverse);
        let cindex = itaruby_semantic::project_index(&cdb);
        assert!(cindex.resolve_const_through_aliases(&["Outer".into(), "Outer::SharedSuite".into()], "Known").is_none());
        let a = cfiles.iter().copied().find(|f| index_path_ends(&cdb, *f, "a.rb")).unwrap();
        let cdiags = itaruby_semantic::check_file(&cdb, a);
        assert!(cdiags.is_empty(), "a shadowed dynamic value under conflict must not accuse: {cdiags:?}");
    }
}

fn index_path_ends(db: &Db, file: SourceFile, suffix: &str) -> bool {
    file.path(db).to_string_lossy().ends_with(suffix)
}
