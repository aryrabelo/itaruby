//! Whole-project suites can declare the same class with incompatible bases.
//! File enumeration must not select the base used for checking or navigation.
use itaruby_semantic::index::MethodLookup;
use itaruby_semantic::{Db, ProjectFiles, SourceFile};

fn project(sources: &[&str], reverse: bool) -> (Db, Vec<SourceFile>) {
    let db = Db::default();
    let files: Vec<_> = sources.iter().enumerate().map(|(i, text)| {
        SourceFile::new(&db, format!("/ancestry_conflict/{i}.rb").into(), (*text).to_string())
    }).collect();
    let mut order = files.clone();
    if reverse {
        order.reverse();
    }
    ProjectFiles::new(&db, order);
    (db, files)
}

const BASES: &str = "
class AlphaBase
  InheritedValue = 1
  class Subscriber; end
  def inherited(value); value; end
  def self.inherited(value); value; end
end
class BetaBase
  def inherited; 0; end
  def self.inherited; 0; end
end
";

#[test]
fn conflicting_bases_never_choose_an_inherited_method_or_constant() {
    let sources = [BASES, "class SharedSuite < AlphaBase; end", "class SharedSuite < BetaBase; end", "
class SharedSuite
  def exercise
    InheritedValue
    Subscriber.new.unknown_call
    inherited(1, 2)
  end
  def forwarding; super; end
end
SharedSuite.inherited(1, 2)
SharedSuite.new.missing_from_both
"];
    for reverse in [false, true] {
        let (db, files) = project(&sources, reverse);
        let index = itaruby_semantic::project_index(&db);
        let file = files[3];
        let diagnostics = itaruby_semantic::check_file(&db, file);
        assert!(diagnostics.is_empty(), "ambiguous ancestry must not diagnose: {diagnostics:?}");
        let id = index.by_path["SharedSuite"];
        assert!(!index.ancestors(id).1, "conflicting ancestry must stay incomplete");
        assert!(matches!(index.lookup_method(id, "missing_from_both"), MethodLookup::Inconclusive));
        assert!(matches!(index.lookup_singleton(id, "inherited"), MethodLookup::Inconclusive));
        assert!(matches!(index.super_lookup(id, false, "inherited"), MethodLookup::Inconclusive));
        assert!(matches!(index.lookup_singleton_own(id, "new"), MethodLookup::Inconclusive));
        assert!(index.const_exists(&["SharedSuite".into()], "InheritedValue"));
        for needle in ["inherited(1, 2)", "super", "SharedSuite.inherited"] {
            let offset = sources[3].find(needle).unwrap() + match needle {
                "SharedSuite.inherited" => "SharedSuite.".len(),
                _ => 0,
            };
            assert!(itaruby_semantic::definition_at(&db, file, offset).is_none(), "no guessed definition for {needle}");
        }
    }
}

#[test]
fn ambiguous_ancestor_never_falls_back_to_a_global_class_or_alias() {
    let sources = [BASES, "class SharedSuite < AlphaBase; end", "class SharedSuite < BetaBase; end", "
class Subscriber
  def wrong_target; 0; end
end
GlobalAlias = Subscriber
class Descendant < SharedSuite
  def exercise
    Subscriber.new.wrong_target(1)
    GlobalAlias.new.wrong_target(1)
    Subscriber::InheritedValue
    GlobalAlias::InheritedValue
  end
end
"];
    for reverse in [false, true] {
        let (db, files) = project(&sources, reverse);
        let index = itaruby_semantic::project_index(&db);
        let nesting = ["Descendant".into()];
        for name in ["Subscriber", "GlobalAlias"] {
            assert!(index.resolve_const_through_aliases(&nesting, name).is_none(), "ambiguous name must not become a global type");
            let offset = sources[3].find(&format!("{name}.new.wrong_target")).unwrap() + name.len() + ".new.".len();
            assert!(itaruby_semantic::definition_at(&db, files[3], offset).is_none(), "ambiguous receiver must not navigate to a global class's method");
        }
        assert_eq!(index.resolve_const(&nesting, "::Subscriber"), Some(index.by_path["Subscriber"]));
        let diagnostics = itaruby_semantic::check_file(&db, files[3]);
        assert!(diagnostics.is_empty(), "unknown constant receivers must stay silent: {diagnostics:?}");
    }
}

#[test]
fn repeated_same_base_and_distinct_spellings_preserve_lookup_and_diagnostics() {
    let sources = ["
module Container
  class Base
    VALUE = 1
    def inherited(value); value; end
    def self.inherited(value); value; end
  end
  class Child < Base; end
end
", "class Container::Child < ::Container::Base; end", "class Container::Child < Container::Base; end\nclass Container::Child < Container::Base; end", "
class Container::Child
  def exercise; VALUE; end
end
Container::Child.new.inherited(1, 2)
Container::Child.inherited(1)
"];
    for reverse in [false, true] {
        let (db, files) = project(&sources, reverse);
        let index = itaruby_semantic::project_index(&db);
        let child = index.by_path["Container::Child"];
        let base = index.by_path["Container::Base"];
        assert!(index.ancestors(child).1);
        for lookup in [index.lookup_method(child, "inherited"), index.lookup_singleton(child, "inherited"), index.super_lookup(child, false, "inherited")] {
            match lookup {
                MethodLookup::Found(method, owner) => {
                    assert_eq!(owner, base);
                    assert_eq!(method.required, 1);
                }
                other => panic!("same base must still resolve: {other:?}"),
            }
        }
        let diagnostics = itaruby_semantic::check_file(&db, files[3]);
        assert_eq!(diagnostics.len(), 1, "normal arity still checks: {diagnostics:?}");
        assert_eq!(diagnostics[0].code, "E0102");
        let offset = sources[3].find("new.inherited").unwrap() + "new.".len();
        let site = itaruby_semantic::definition_at(&db, files[3], offset).expect("same-base method still navigates");
        assert_eq!(site.file, files[0]);
    }
}

#[test]
fn normal_and_unresolved_bases_keep_their_existing_contract() {
    let sources = [BASES, "class Ordinary < AlphaBase; end", "class Uncertain < ExternalUnseen; end", "class Uncertain < ExternalUnseen; end", "
Ordinary.new.inherited(1, 2)
Ordinary.new.no_such_method
Uncertain.new.no_such_method
Ordinary::MISSING_VALUE
"];
    for reverse in [false, true] {
        let (db, files) = project(&sources, reverse);
        let index = itaruby_semantic::project_index(&db);
        assert!(matches!(index.lookup_method(index.by_path["Ordinary"], "no_such_method"), MethodLookup::NotFound));
        assert!(matches!(index.lookup_method(index.by_path["Uncertain"], "no_such_method"), MethodLookup::Inconclusive));
        let diagnostics = itaruby_semantic::check_file(&db, files[4]);
        let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(codes, ["E0102", "E0101", "E0104"]);
    }
}

#[test]
fn superclass_via_conflicting_enclosing_ancestry_has_no_global_winner() {
    let sources = [BASES, "class SharedSuite < AlphaBase; end", "class SharedSuite < BetaBase; end", "
class Subscriber
  def wrong_winner(value); value; end
end
class SharedSuite
  class Nested < Subscriber; end
end
SharedSuite::Nested.new.wrong_winner(1, 2)
"];
    for reverse in [false, true] {
        let (db, files) = project(&sources, reverse);
        let index = itaruby_semantic::project_index(&db);
        let child = index.by_path["SharedSuite::Nested"];
        assert!(matches!(index.lookup_method(child, "wrong_winner"), MethodLookup::Inconclusive));
        let diagnostics = itaruby_semantic::check_file(&db, files[3]);
        assert!(diagnostics.is_empty(), "ambiguous superclass cannot supply arity: {diagnostics:?}");
        let offset = sources[3].find("new.wrong_winner").unwrap() + "new.".len();
        assert!(itaruby_semantic::definition_at(&db, files[3], offset).is_none());
    }
}

#[test]
fn a_superclass_header_keeps_its_own_nesting_after_a_bare_reopening() {
    let sources = ["
module Lexical
  class Base
    def inherited(value); value; end
  end
  class Child < Base; end
end
", "class Lexical::Child; end", "Lexical::Child.new.inherited(1, 2)"];
    for reverse in [false, true] {
        let (db, files) = project(&sources, reverse);
        let index = itaruby_semantic::project_index(&db);
        let child = index.by_path["Lexical::Child"];
        match index.lookup_method(child, "inherited") {
            MethodLookup::Found(_, owner) => assert_eq!(owner, index.by_path["Lexical::Base"]),
            other => panic!("header nesting must survive a reopening: {other:?}"),
        }
        let diagnostics = itaruby_semantic::check_file(&db, files[2]);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].code, "E0102");
    }
}

#[test]
fn identical_base_text_in_different_scopes_conflicts_but_local_methods_remain_known() {
    let sources = ["
class Base
  def inherited(value); value; end
end
module Outer
  class Base
    def inherited; 0; end
  end
  class Child < Base
    def local(value); value; end
  end
end
", "class Outer::Child < Base; end", "
Outer::Child.new.inherited(1, 2)
Outer::Child.new.local(1, 2)
"];
    for reverse in [false, true] {
        let (db, files) = project(&sources, reverse);
        let index = itaruby_semantic::project_index(&db);
        let child = index.by_path["Outer::Child"];
        assert!(matches!(index.lookup_method(child, "inherited"), MethodLookup::Inconclusive));
        assert!(matches!(index.lookup_method(child, "local"), MethodLookup::Found(_, _)));
        let diagnostics = itaruby_semantic::check_file(&db, files[2]);
        assert_eq!(diagnostics.len(), 1, "only the local arity is known: {diagnostics:?}");
        assert_eq!(diagnostics[0].code, "E0102");
        let local_call = sources[2].find("Outer::Child.new.local").unwrap();
        assert!((local_call..sources[2].len()).contains(&diagnostics[0].start));
    }
}

#[test]
fn schema_synthesis_cannot_use_a_conflicting_superclass() {
    let sources = [
        ("bases.rb", "class ApplicationRecord; end\nclass OtherBase; end\nclass OrdinaryRecord < ApplicationRecord; end"),
        ("a.rb", "class SharedSuite < ApplicationRecord; end"),
        ("b.rb", "class SharedSuite < OtherBase; end"),
        ("db/schema.rb", "ActiveRecord::Schema.define do\ncreate_table 'shared_suites' do |t|\nt.string 'title'\nend\ncreate_table 'ordinary_records' do |t|\nt.string 'title'\nend\nend"),
        ("calls.rb", "SharedSuite.new.title(1)\nOrdinaryRecord.new.title(1)"),
    ];
    for reverse in [false, true] {
        let db = Db::default();
        let files: Vec<_> = sources.iter().map(|(name, text)| {
            SourceFile::new(&db, format!("/ancestry_schema/{name}").into(), (*text).to_string())
        }).collect();
        let mut order = files.clone();
        if reverse {
            order.reverse();
        }
        ProjectFiles::new(&db, order);
        let index = itaruby_semantic::project_index(&db);
        assert!(matches!(index.lookup_method(index.by_path["SharedSuite"], "title"), MethodLookup::Inconclusive));
        assert!(matches!(index.lookup_method(index.by_path["OrdinaryRecord"], "title"), MethodLookup::Found(_, _)));
        let diagnostics = itaruby_semantic::check_file(&db, files[4]);
        assert_eq!(diagnostics.len(), 1, "only the ordinary model has a proven attribute: {diagnostics:?}");
        assert_eq!(diagnostics[0].code, "E0102");
        let ordinary = sources[4].1.find("OrdinaryRecord").unwrap();
        assert!((ordinary..sources[4].1.len()).contains(&diagnostics[0].start));
    }
}
