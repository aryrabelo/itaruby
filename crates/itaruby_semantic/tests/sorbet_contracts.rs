//! End-to-end shared CLI/LSP checker contracts. Mutation probes for this
//! instrument: `python3 scripts/sorbet-contracts-mutants.py` (`--anchors`
//! counts every needle without building).
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
class ContractSplatPair
  extend T::Sig
  sig { params(count: Integer, label: String).returns(String) }
  def pair(count, label)
    label
  end
end
ContractSplatPair.new.pair(*[1], "good")
"#, None);
    // After a splat no later positional is proven to land on any name:
    // `"good"` is really `label` here, never `count`.
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

/// The body returns an Integer against the stale RBI's `returns(String)`:
/// were the renamed layout accepted, E0109 would accuse it. Returning the
/// untyped parameter instead would stay silent either way and prove nothing.
#[test]
fn stale_rbi_layout_does_not_type_source_or_calls() {
    let diags = check("stale", r"
class ContractStale
  def echo(actual)
    1
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

/// A subclass that redefines the method takes the parent's signature off
/// every call that could dispatch to either body: the sig is no longer
/// proven to govern the call, so neither the argument nor the parent body
/// is judged by it. The control — the same parent without the override —
/// keeps this a contract rather than a blind spot.
#[test]
fn descendant_override_keeps_the_parent_contract_off_the_call() {
    const PARENT: &str = r#"
class ContractOverrideParent
  extend T::Sig
  sig { params(value: Integer).returns(Integer) }
  def echo(value)
    "wrong"
  end
end
ContractOverrideParent.new.echo("bad")
"#;
    let overridden = format!("{PARENT}class ContractOverrideChild < ContractOverrideParent\n  def echo(value)\n    value\n  end\nend\n");
    let diags = check("descendant-override", &overridden, None);
    assert!(contract_codes(&diags).is_empty(), "{diags:?}");

    let control = check("descendant-override-control", PARENT, None);
    assert_eq!(contract_codes(&control), ["E0109", "E0103"], "control must accuse without the override: {control:?}");
}

/// An RBI contract belongs to the exact owner it is written on. A method
/// the RBI declares on the parent is not the child's source override,
/// even though the RBI closure of the child reaches it.
#[test]
fn inherited_rbi_contract_never_attaches_to_a_source_override() {
    let diags = check("inherited-rbi", r#"
class ContractRbiParent
end
class ContractRbiChild < ContractRbiParent
  def echo(value)
    "wrong"
  end
end
ContractRbiChild.new.echo("bad")
"#, Some(r"
class ContractRbiParent
  sig { params(value: Integer).returns(Integer) }
  def echo(value); end
end
class ContractRbiChild < ContractRbiParent
end
"));
    assert!(contract_codes(&diags).is_empty(), "{diags:?}");
}

/// A sig whose `params` names a parameter the def does not have is not a
/// contract for that def: nothing it says is guessed onto the remaining
/// names, so neither the argument nor the body is accused.
#[test]
fn sig_naming_an_absent_parameter_is_not_a_contract() {
    let diags = check("absent-param", r#"
class ContractAbsentParam
  extend T::Sig
  sig { params(value: Integer, extra: String).returns(Integer) }
  def echo(value)
    "wrong"
  end
end
ContractAbsentParam.new.echo("bad")
"#, None);
    assert!(contract_codes(&diags).is_empty(), "{diags:?}");
}

/// An inline sig this checker cannot use still means somebody annotated
/// the definition, so a client RBI must not step in with its own contract.
/// Stacked sigs are the unusable spelling that reaches this decision: an
/// unrecognized spelling such as `sig(:abstract)` already opens the owner,
/// which takes every contract off it before the RBI is consulted.
#[test]
fn unusable_inline_sig_still_blocks_the_rbi_contract() {
    let diags = check("unusable-inline", r#"
class ContractUnusableInline
  extend T::Sig
  sig { params(value: String).returns(String) }
  sig { params(value: Integer).returns(Integer) }
  def echo(value)
    "wrong"
  end
end
ContractUnusableInline.new.echo("bad")
"#, Some(r"
class ContractUnusableInline
  sig { params(value: Integer).returns(Integer) }
  def echo(value); end
end
"));
    assert!(contract_codes(&diags).is_empty(), "{diags:?}");
}

/// A `T.nilable` parameter seeds `Integer | nil` into the body, and this
/// checker does not strip nil through `||`, `||=` or the usual guards. Each
/// method below returns a value that is never nil at runtime, so nil
/// membership in the inferred union is unproven and must not accuse.
#[test]
fn nilable_return_through_defaults_and_guards_stays_silent() {
    let diags = check("nilable-return", r"
class ContractNilableReturn
  extend T::Sig
  def initialize
    @count = nil
  end
  def load_count
    @count = 3
  end
  sig { params(x: T.nilable(Integer)).returns(Integer) }
  def or_default(x)
    x || 0
  end
  sig { params(x: T.nilable(Integer)).returns(Integer) }
  def or_assign(x)
    x ||= 0
    x
  end
  sig { returns(Integer) }
  def local_or_assign
    r = nil
    r ||= 1
    r
  end
  sig { params(x: T.nilable(Integer)).returns(Integer) }
  def blank_guard(x)
    return 0 if x.blank?
    x
  end
  sig { params(x: T.nilable(Integer)).returns(Integer) }
  def and_guard(x)
    return 0 unless x && x > 0
    x
  end
  sig { params(x: T.nilable(Integer)).returns(Integer) }
  def is_a_guard(x)
    unless x.is_a?(Integer)
      return 0
    end
    x
  end
  sig { params(x: T.nilable(Integer)).returns(Integer) }
  def eq_nil_guard(x)
    if x == nil
      return 0
    end
    x
  end
  sig { params(x: T.nilable(Integer)).returns(Integer) }
  def bang_guard(x)
    if !x
      return 0
    end
    x
  end
  sig { params(x: T.nilable(Integer)).returns(Integer) }
  def paren_return_guard(x)
    (return 0) unless x
    x
  end
  sig { returns(Integer) }
  def ivar_guard
    return 0 unless @count
    @count
  end
  sig { params(x: T.nilable(Integer)).returns(Integer) }
  def explicit_return_after_guard(x)
    return x if x
    0
  end
  sig { params(x: T.nilable(Integer)).returns(T::Array[Integer]) }
  def wrapped_in_array(x)
    return [] if x.nil?
    [x]
  end
end
", None);
    assert!(contract_codes(&diags).is_empty(), "{diags:?}");
}

/// The E0103 side of the same false positive: the caller's own
/// `T.nilable(String)` parameter reaches a non-nilable parameter only
/// after a default or a guard this checker does not narrow through.
#[test]
fn nilable_argument_through_defaults_and_guards_stays_silent() {
    let diags = check("nilable-argument", r#"
class ContractNilableArgument
  extend T::Sig
  def initialize
    @label = nil
  end
  def load_label
    @label = "loaded"
  end
  sig { params(label: String).returns(String) }
  def shout(label)
    label
  end
  sig { params(name: T.nilable(String)).returns(String) }
  def or_default(name)
    shout(name || "anon")
  end
  sig { params(name: T.nilable(String)).returns(String) }
  def or_assign(name)
    name ||= "anon"
    shout(name)
  end
  sig { returns(String) }
  def local_or_assign
    r = nil
    r ||= "anon"
    shout(r)
  end
  sig { params(name: T.nilable(String)).returns(String) }
  def blank_guard(name)
    return "anon" if name.blank?
    shout(name)
  end
  sig { params(name: T.nilable(String)).returns(String) }
  def and_guard(name)
    return "anon" unless name && name.size > 0
    shout(name)
  end
  sig { params(name: T.nilable(String)).returns(String) }
  def is_a_guard(name)
    unless name.is_a?(String)
      return "anon"
    end
    shout(name)
  end
  sig { params(name: T.nilable(String)).returns(String) }
  def eq_nil_guard(name)
    if name == nil
      return "anon"
    end
    shout(name)
  end
  sig { params(name: T.nilable(String)).returns(String) }
  def bang_guard(name)
    if !name
      return "anon"
    end
    shout(name)
  end
  sig { params(name: T.nilable(String)).returns(String) }
  def paren_return_guard(name)
    (return "anon") unless name
    shout(name)
  end
  sig { returns(String) }
  def ivar_guard
    return "anon" unless @label
    shout(@label)
  end
end
"#, None);
    assert!(contract_codes(&diags).is_empty(), "{diags:?}");
}

/// A bare `nil` read back from a local or an instance variable is a flow
/// fact, not a proof: writes this checker cannot see (an attribute writer,
/// a block, reflection) may have replaced it. Only a `nil` written where
/// the value is consumed proves it. The same holds for a local: a nil local
/// that reaches a contract unchanged in straight-line code is a real bug
/// this rule gives up (a false negative, acceptable under invariant #1);
/// the block-written local is the shape where the read is NOT the value.
#[test]
fn nil_read_from_a_variable_is_not_proof() {
    let diags = check("nil-variable", r"
class ContractNilVariable
  extend T::Sig
  attr_writer :label
  def initialize
    @label = nil
  end
  sig { params(label: String).returns(String) }
  def shout(label)
    label
  end
  sig { returns(String) }
  def ivar_argument
    shout(@label)
  end
  sig { returns(String) }
  def ivar_return
    @label
  end
  def current_label
    @label
  end
  sig { returns(String) }
  def call_result_argument
    shout(current_label)
  end
  sig { params(items: T::Array[String]).returns(String) }
  def block_written_local(items)
    value = nil
    items.each { |item| value = item }
    shout(value)
  end
end
", None);
    assert!(contract_codes(&diags).is_empty(), "{diags:?}");
}

/// Controls that keep the nil rule a contract and not a blind spot: a
/// literal `nil` in return or argument position still accuses, and so
/// does a union whose non-nil member is itself a mismatch.
#[test]
fn literal_nil_and_mismatched_union_members_still_accuse() {
    let source = r#"
class ContractNilLiteral
  extend T::Sig
  sig { params(label: String).returns(String) }
  def shout(label)
    label
  end
  sig { returns(Integer) }
  def tail_nil
    nil
  end
  sig { params(flag: T.untyped).returns(Integer) }
  def return_nil(flag)
    return nil if flag
    1
  end
  sig { params(flag: T.untyped).returns(Integer) }
  def mixed_union(flag)
    flag ? "wrong" : nil
  end
end
ContractNilLiteral.new.shout(nil)
"#;
    let diags = check("nil-literal", source, None);
    let names: Vec<&str> = diags.iter().filter(|d| matches!(d.code, "E0103" | "E0109"))
        .map(|d| &source[d.start..d.end]).collect();
    assert_eq!(contract_codes(&diags), ["E0109", "E0109", "E0109", "E0103"], "{diags:?}");
    assert_eq!(names, ["tail_nil", "return_nil", "mixed_union", "nil"], "{diags:?}");
}
