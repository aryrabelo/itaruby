//! End-to-end shared CLI/LSP checker contracts. Mutation probes live beside
//! this instrument: `python3 crates/itaruby_semantic/tests/sorbet_contracts_mutants.py`.
//! No Sorbet runtime, generated RBI refresh, or private corpus is required.

use itaruby_semantic::{check_file, ClosedWorld, Db, Diagnostic, ProjectFiles, RbiProject, SourceFile};

fn check(name: &str, source: &str, rbi: Option<&str>) -> Vec<Diagnostic> {
    let db = Db::default();
    let root = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("sorbet-contract-{name}"));
    if let Some(text) = rbi {
        let dir = root.join("sorbet/rbi");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("contract.rbi");
        std::fs::write(&file, text).unwrap();
        let declarations = itaruby_semantic::rbi::build_rbi_index(&[file]);
        RbiProject::new(&db, declarations.constants);
    }
    let file = SourceFile::new(&db, root.join("source.rb"), source.to_owned());
    ProjectFiles::new(&db, vec![file]);
    ClosedWorld::new(&db, true);
    check_file(&db, file).clone()
}

fn contract_codes(diags: &[Diagnostic]) -> Vec<&str> {
    diags.iter().filter(|d| matches!(d.code, "E0103" | "E0109")).map(|d| d.code).collect()
}

#[test]
fn named_positional_params_accuse_at_argument() {
    let source = r#"
class ContractNamed
  extend T::Sig
  sig { params(label: String, count: Integer).returns(String) }
  def build(count, label)
    label
  end
end
ContractNamed.new.build(1, "good")
ContractNamed.new.build("bad", "good")
"#;
    let diags = check("named", source, None);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, "E0103");
    assert_eq!(&source[diags[0].start..diags[0].end], "\"bad\"");
    assert!(diags[0].message.contains("count"), "{diags:?}");
}

#[test]
fn keywords_and_defaults_keep_named_correspondence() {
    let diags = check("keywords", r#"
class ContractKeywords
  extend T::Sig
  sig { params(label: String, count: Integer).returns(String) }
  def build(count = 1, label:)
    label
  end
end
ContractKeywords.new.build(label: "good")
ContractKeywords.new.build(2, label: "good")
ContractKeywords.new.build(label: 3)
ContractKeywords.new.build(*["unknown position"], label: "good")
ContractKeywords.new.build(label: "ignored", label: "last")
"#, None);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, "E0103");
    assert!(diags[0].message.contains("label"), "{diags:?}");
}

#[test]
fn signature_params_bind_the_source_body() {
    let diags = check("binding", r"
class ContractBoundValue
end
class ContractBinding
  extend T::Sig
  sig { params(value: ContractBoundValue).returns(Integer) }
  def inspect_value(value)
    value.absent_bound_method
    1
  end
end
", None);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, "E0101");
    assert!(diags[0].message.contains("absent_bound_method"), "{diags:?}");
}

#[test]
fn incompatible_source_return_accuses_without_a_call() {
    let source = r#"
class ContractBadReturn
  extend T::Sig
  sig { returns(Integer) }
  def amount
    "wrong"
  end
end
"#;
    let diags = check("return", source, None);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, "E0109");
    assert_eq!(&source[diags[0].start..diags[0].end], "amount");
}

#[test]
fn explicit_returns_and_implicit_branches_are_checked() {
    let diags = check("branches", r#"
class ContractBranches
  extend T::Sig
  sig { params(flag: T.untyped).returns(Integer) }
  def explicit(flag)
    if flag
      return "wrong"
    else
      return 1
    end
  end
  sig { params(flag: T.untyped).returns(Integer) }
  def implicit(flag)
    if flag
      "wrong"
    else
      1
    end
  end
end
"#, None);
    assert_eq!(contract_codes(&diags), ["E0109", "E0109"], "{diags:?}");
}

#[test]
fn void_unknown_and_uncertain_return_paths_stay_silent() {
    let diags = check("return-controls", r#"
class ContractReturnControls
  extend T::Sig
  sig { void }
  def save
    "a value is allowed"
  end
  sig { params(value: T.untyped).returns(Integer) }
  def opaque(value)
    value
  end
  sig { abstract.returns(Integer) }
  def abstract_value; end
  sig { returns(Integer) }
  def ensure_overrides
    begin
      "discarded"
    ensure
      return 1
    end
  end
  sig { returns(Integer) }
  def dead_suffix
    return 1
    "not returned"
  end
  sig { returns(Integer) }
  def unreachable_branch
    if false
      "not returned"
    else
      1
    end
  end
end
"#, None);
    assert!(contract_codes(&diags).is_empty(), "{diags:?}");
}

/// An RBI informs the call without replacing the checks the Ruby source
/// already earned: the arity check on `plain` survives beside the two new
/// contract diagnostics.
///
/// `absent_ruby_method` is deliberately NOT asserted here. A `.rbi` naming
/// a class the project also defines is read as a gem reopening
/// (`soften_not_found`'s `gem_reopens`, bead ita-oaq), so the project's
/// view of that class is provably partial and `NotFound` softens to
/// `Inconclusive`. Measured 2026-09-22 against `origin/main` at 6d04dc7,
/// with zero signature code in the tree: the same source plus the same
/// `.rbi` already lost that E0101 there, and the layout does not change it
/// (`sorbet/rbi`, `gems/`, `dsl/` and `shims/` all soften alike). Narrowing
/// that defense to win this one line would trade a measured false-positive
/// guard for a synthetic diagnostic — the wrong direction under invariant
/// #1.
///
/// The control below is what keeps that a CONTRACT rather than a blind
/// spot: without the `.rbi`, the very same call must still accuse.
#[test]
fn rbi_contract_checks_source_without_replacing_ruby_checks() {
    const SOURCE: &str = r#"
class ContractSourceRbi
  def amount(count)
    "wrong"
  end
  def plain; 1; end
end
ContractSourceRbi.new.amount("bad")
ContractSourceRbi.new.plain(1)
ContractSourceRbi.new.absent_ruby_method
"#;
    let diags = check("source-rbi", SOURCE, Some(r"
class ContractSourceRbi
  sig { params(count: Integer).returns(Integer) }
  def amount(count); end
end
"));
    assert_eq!(contract_codes(&diags), ["E0109", "E0103"], "{diags:?}");
    assert!(diags.iter().any(|d| d.code == "E0102" && d.message.contains("plain")), "{diags:?}");
    assert_eq!(diags.len(), 3, "{diags:?}");

    let control = check("source-rbi-control", SOURCE, None);
    assert!(
        control.iter().any(|d| d.code == "E0101" && d.message.contains("absent_ruby_method")),
        "control must still accuse the absent method without the rbi: {control:?}"
    );
}

#[test]
fn rbi_return_informs_consumers_and_never_checks_empty_declaration() {
    let diags = check("rbi-return", r"
class ContractRbiResult
end
class ContractRbiFactory
  def make(input)
    input
  end
end
ContractRbiFactory.new.make(nil).absent_result_method
", Some(r"
class ContractRbiFactory
  sig { params(input: T.untyped).returns(ContractRbiResult) }
  def make(input); end
end
"));
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, "E0101");
    assert!(diags[0].message.contains("absent_result_method"), "{diags:?}");
}

#[test]
fn external_rbi_params_are_checked_on_a_single_proven_edge() {
    let diags = check("external-rbi", r#"
class ContractExternalSite < ContractExternalBase
  def initialize; end
end
ContractExternalSite.new.accept("bad")
ContractExternalSite.new.accept(1)
"#, Some(r"
class ContractExternalBase
  sig { params(value: Integer).returns(String) }
  def accept(value); end
end
"));
    assert_eq!(contract_codes(&diags), ["E0103"], "{diags:?}");
    assert!(!diags.iter().any(|d| d.code == "E0109"), "{diags:?}");
}

#[test]
fn source_inline_contract_wins_over_rbi() {
    let diags = check("source-precedence", r"
class ContractSourceWins
  extend T::Sig
  sig { params(value: Integer).returns(Integer) }
  def echo(value)
    value
  end
end
ContractSourceWins.new.echo(1)
", Some(r"
class ContractSourceWins
  sig { params(value: String).returns(String) }
  def echo(value); end
end
"));
    assert!(diags.is_empty(), "{diags:?}");
}

#[test]
fn stale_rbi_layout_does_not_type_source_or_calls() {
    let diags = check("stale", r"
class ContractStale
  def echo(actual)
    actual
  end
end
ContractStale.new.echo(1)
", Some(r"
class ContractStale
  sig { params(outdated: String).returns(String) }
  def echo(outdated); end
end
"));
    assert!(diags.is_empty(), "{diags:?}");
}

#[test]
fn conflicting_rbi_declarations_do_not_choose_a_contract() {
    let diags = check("conflicting-rbi", r"
class ContractConflict
  def echo(value)
    value
  end
end
ContractConflict.new.echo(nil)
", Some(r"
class ContractConflict
  sig { params(value: String).returns(String) }
  def echo(value); end
  sig { params(value: Integer).returns(Integer) }
  def echo(value); end
end
"));
    assert!(diags.is_empty(), "{diags:?}");
}

#[test]
fn overloads_and_unsupported_layouts_do_not_invent_correspondence() {
    let diags = check("unsupported", r#"
class ContractUnsupported
  extend T::Sig
  sig { params(value: String).returns(String) }
  sig { params(value: Integer).returns(Integer) }
  def overloaded(value)
    value
  end
  sig { params(first: Integer, last: String).returns(Integer) }
  def posts(first = 1, last)
    "runtime inference only"
  end
  sig { params(values: Integer).returns(Integer) }
  def rest(*values)
    "runtime inference only"
  end
  sig { params(value: Integer, block: T.untyped).returns(Integer) }
  def callback(value, &block)
    "runtime inference only"
  end
end
ContractUnsupported.new.overloaded(nil)
ContractUnsupported.new.posts("last")
ContractUnsupported.new.rest("values")
ContractUnsupported.new.callback("value") {}
"#, None);
    assert!(contract_codes(&diags).is_empty(), "{diags:?}");
}

#[test]
fn lexical_nominal_types_and_subclasses_use_definition_scope() {
    let diags = check("lexical", r"
class ContractGlobalItem
end
module ContractLexical
  class Item
  end
  class Child < Item
  end
  class Factory
    extend T::Sig
    sig { params(value: Item).returns(Item) }
    def echo(value)
      value
    end
  end
end
ContractLexical::Factory.new.echo(ContractLexical::Child.new)
ContractLexical::Factory.new.echo(ContractGlobalItem.new)
", None);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, "E0103");
    assert!(diags[0].message.contains("ContractLexical::Item"), "{diags:?}");
}

#[test]
fn inherited_and_singleton_signatures_are_not_caller_scoped() {
    let diags = check("tracks", r#"
class ContractTrackBase
  extend T::Sig
  sig { params(value: Integer).returns(Integer) }
  def echo(value); value; end
  sig { params(value: Integer).returns(Integer) }
  def self.parse(value); value; end
end
class ContractTrackChild < ContractTrackBase
end
ContractTrackChild.new.echo("bad")
ContractTrackChild.parse("bad")
ContractTrackChild.new.echo(1)
ContractTrackChild.parse(1)
"#, None);
    assert_eq!(contract_codes(&diags), ["E0103", "E0103"], "{diags:?}");
}

#[test]
fn rbi_singleton_contract_matches_source_track() {
    let diags = check("rbi-singleton", r#"
class ContractRbiSingleton
  def self.parse(value); value; end
end
ContractRbiSingleton.parse("bad")
ContractRbiSingleton.parse(1)
"#, Some(r"
class ContractRbiSingleton
  sig { params(value: Integer).returns(Integer) }
  def self.parse(value); end
end
"));
    assert_eq!(contract_codes(&diags), ["E0103"], "{diags:?}");
}

#[test]
fn type_member_never_resolves_to_a_homonymous_global_class() {
    let diags = check("type-member", r#"
class ContractElement
end
class ContractTypeMember
  extend T::Sig
  ContractElement = type_member
  sig { params(value: ContractElement).returns(ContractElement) }
  def echo(value); value; end
end
ContractTypeMember.new.echo("any type")
"#, None);
    assert!(contract_codes(&diags).is_empty(), "{diags:?}");
}

#[test]
fn unknown_array_elements_preserve_known_array_checks() {
    let diags = check("partial-array", r"
class ContractArray
  extend T::Sig
  sig { params(value: T.untyped).returns(T::Array[T.untyped]) }
  def items(value)
    value
  end
end
ContractArray.new.items(nil).absent_array_method
", None);
    assert!(diags.iter().any(|d| d.code == "E0101" && d.message.contains("absent_array_method")), "{diags:?}");
    assert!(contract_codes(&diags).is_empty(), "{diags:?}");
}

#[test]
fn open_source_keeps_existing_ruby_checks_without_contract_accusations() {
    let diags = check("open", r#"
class ContractOpen
  extend T::Sig
  define_method(dynamic_name) {}
  sig { params(value: Integer).returns(Integer) }
  def echo(value)
    1 + "bad operand"
    "unprovable dispatch"
  end
end
ContractOpen.new.echo("unknown")
"#, None);
    assert!(contract_codes(&diags).is_empty(), "{diags:?}");
    assert!(diags.iter().any(|d| d.code == "E0108"), "{diags:?}");
}

#[test]
fn duplicate_source_definitions_do_not_pick_a_signature() {
    let diags = check("duplicate-source", r"
class ContractDuplicate
  extend T::Sig
  sig { params(value: Integer).returns(Integer) }
  def echo(value); value; end
  sig { params(value: String).returns(String) }
  def echo(value); value; end
end
ContractDuplicate.new.echo(nil)
", None);
    assert!(contract_codes(&diags).is_empty(), "{diags:?}");
}
