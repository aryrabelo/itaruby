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

/// Each explicit `return` is judged on its own; the implicit branches fold
/// into one union, which accuses only when no member fits (see
/// `union_with_one_compatible_member_stays_silent`) — so the implicit
/// control mismatches in BOTH branches.
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
      :wrong
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
# No writer path the index can see: the ivar still infers as nil, and the
# contract still refuses to call a nil read back from it proven.
class ContractNilBareIvar
  extend T::Sig
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
end
", None);
    assert!(contract_codes(&diags).is_empty(), "{diags:?}");
}

/// Controls that keep the nil rule a contract and not a blind spot: a
/// literal `nil` in return or argument position still accuses, and so
/// does a union whose non-nil member is itself a mismatch — the unproven
/// `nil` is dropped from the union, never read as an alibi for it.
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

fn contract_names<'a>(source: &'a str, diags: &[Diagnostic]) -> Vec<&'a str> {
    diags.iter().filter(|d| matches!(d.code, "E0103" | "E0109")).map(|d| &source[d.start..d.end]).collect()
}

/// A project reopen of a core class/module is still the core constant: a
/// bare core name in a sig keeps its scalar meaning or stays `Unknown`, and
/// never becomes a project `Instance` that no literal value can satisfy.
/// The reopened `Integer` control keeps the scalar contract live.
#[test]
fn reopened_core_names_keep_their_core_meaning() {
    let source = r#"
class Hash
  def contract_core_touch; self; end
end
class Array
  def contract_core_touch; self; end
end
class Object
  def contract_core_touch; self; end
end
class BasicObject
end
class Numeric
end
module Comparable
end
module Enumerable
end
module Kernel
end
class Integer
  def contract_core_touch; self; end
end
class ContractCoreReopen
  extend T::Sig
  sig { params(value: Hash).returns(Hash) }
  def hash_echo(value)
    { a: 1 }
  end
  sig { params(value: Array).returns(Array) }
  def array_echo(value)
    [1, 2]
  end
  sig { params(value: Object).returns(Object) }
  def object_echo(value)
    1
  end
  sig { params(value: BasicObject).returns(BasicObject) }
  def basic_echo(value)
    "text"
  end
  sig { params(value: Numeric).returns(Numeric) }
  def numeric_echo(value)
    1.5
  end
  sig { params(value: Comparable).returns(Comparable) }
  def comparable_echo(value)
    "text"
  end
  sig { params(value: Enumerable).returns(Kernel) }
  def enumerable_echo(value)
    :sym
  end
  sig { returns(Integer) }
  def still_integer
    "not an integer"
  end
end
reopen = ContractCoreReopen.new
reopen.hash_echo({ a: 1 })
reopen.array_echo([1])
reopen.object_echo(1)
reopen.basic_echo(:sym)
reopen.numeric_echo(2)
reopen.comparable_echo("x")
reopen.enumerable_echo([1])
"#;
    let diags = check("core-reopen", source, None);
    assert_eq!(contract_codes(&diags), ["E0109"], "{diags:?}");
    assert_eq!(contract_names(source, &diags), ["still_integer"], "{diags:?}");
}

/// A project module mixed into a core class is satisfied by core values the
/// checker cannot relate to it: the name stays `Unknown`. A project class
/// that only a project class includes keeps its contract.
#[test]
fn project_module_mixed_into_core_never_accuses_core_values() {
    let source = r"
module ContractCoreMixin
end
class Integer
  include ContractCoreMixin
end
class ContractMixinTarget
end
class ContractMixinUser
  extend T::Sig
  sig { params(value: ContractCoreMixin).returns(ContractCoreMixin) }
  def echo(value)
    1
  end
  sig { returns(ContractMixinTarget) }
  def target
    2
  end
end
ContractMixinUser.new.echo(1)
";
    let diags = check("core-mixin", source, None);
    assert_eq!(contract_codes(&diags), ["E0109"], "{diags:?}");
    assert_eq!(contract_names(source, &diags), ["target"], "{diags:?}");
}

/// Ruby looks a constant up in the ancestors of the cref before the top
/// level. A name some ancestor namespace could answer, or any name inside a
/// class whose ancestry is not fully known, is `Unknown`; a fully known,
/// unshadowed ancestry keeps the top-level binding and its contract.
#[test]
fn inherited_namespace_constants_never_bind_to_top_level() {
    let source = r#"
class ContractNode
end
class ContractParent
  class ContractNode
  end
  class String
  end
end
class ContractKid < ContractParent
  extend T::Sig
  sig { params(value: ContractNode).returns(ContractNode) }
  def echo(value)
    value
  end
  sig { returns(ContractNode) }
  def build
    ContractParent::ContractNode.new
  end
  sig { returns(String) }
  def text
    ContractParent::String.new
  end
end
module ContractMix
  class ContractLeaf
  end
end
class ContractLeaf
end
class ContractIncluder
  extend T::Sig
  include ContractMix
  sig { returns(ContractLeaf) }
  def leaf
    ContractMix::ContractLeaf.new
  end
end
class ContractUnknownKid < ContractUnresolvedBase
  extend T::Sig
  sig { returns(ContractNode) }
  def build
    "anything"
  end
end
class ContractPlainParent
end
class ContractPlainKid < ContractPlainParent
  extend T::Sig
  sig { returns(ContractNode) }
  def build
    "not a node"
  end
end
ContractKid.new.echo(ContractParent::ContractNode.new)
"#;
    let diags = check("ancestor-namespace", source, None);
    assert_eq!(contract_codes(&diags), ["E0109"], "{diags:?}");
    assert_eq!(contract_names(source, &diags), ["build"], "{diags:?}");
    let plain = source.find("class ContractPlainKid").unwrap();
    assert!(diags.iter().filter(|d| d.code == "E0109").all(|d| d.start > plain), "{diags:?}");
}

// -- core returns follow their arguments (a wrong precise type accuses) --

fn codes(diags: &[Diagnostic]) -> Vec<&str> {
    diags.iter().map(|d| d.code).collect()
}

#[test]
fn integer_arithmetic_takes_the_operand_type() {
    let diags = check("int-operand", r"
class ContractIntOperand
  extend T::Sig
  sig { params(x: Integer).returns(Float) }
  def half(x)
    x + 0.5
  end
  sig { params(x: Integer, y: T.untyped).returns(Float) }
  def scaled(x, y)
    x * y
  end
  sig { params(x: Integer).returns(T::Boolean) }
  def ratio(x)
    (x * 1.5).nan?
  end
  sig { params(x: Float, y: T.untyped).returns(Integer) }
  def decimal(x, y)
    x * y
  end
  sig { params(x: Integer).returns(Integer) }
  def wrong(x)
    x * 1.5
  end
end
", None);
    // Only the provable Float into an Integer contract accuses.
    assert_eq!(codes(&diags), ["E0109"], "{diags:?}");
    assert!(diags[0].message.contains("got Float"), "{diags:?}");
}

#[test]
fn float_rounding_with_digits_is_not_an_integer() {
    let diags = check("float-digits", r"
class ContractFloatDigits
  extend T::Sig
  sig { params(x: Float).returns(Float) }
  def cents(x)
    x.round(2)
  end
  sig { params(x: Float, n: Integer).returns(Float) }
  def floored(x, n)
    x.floor(n)
  end
  sig { params(x: Float).returns(T::Boolean) }
  def odd_cents(x)
    x.ceil(1).nan?
  end
  sig { params(x: Float).returns(String) }
  def whole(x)
    x.round
  end
end
", None);
    // `round` with no digits still proves an Integer.
    assert_eq!(codes(&diags), ["E0109"], "{diags:?}");
    assert!(diags[0].message.contains("got Integer"), "{diags:?}");
}

#[test]
fn array_count_argument_returns_an_array() {
    let diags = check("array-count", r"
class ContractArrayCount
  extend T::Sig
  sig { params(x: T::Array[Integer]).returns(T::Array[Integer]) }
  def head(x)
    x.first(2)
  end
  sig { params(x: T::Array[Integer]).returns(T::Array[Integer]) }
  def top(x)
    x.max(2)
  end
  sig { params(x: T::Array[Integer]).returns(T.untyped) }
  def pairs(x)
    x.pop(2).each_slice(2)
  end
  sig { params(x: T::Array[Integer], counts: T::Array[Integer]).returns(T::Array[Integer]) }
  def splatted(x, counts)
    x.last(*counts)
  end
  sig { params(x: T::Array[Integer]).returns(String) }
  def one(x)
    x.first
  end
end
", None);
    assert_eq!(codes(&diags), ["E0109"], "{diags:?}");
    assert!(diags[0].message.contains("got Integer"), "{diags:?}");
}

#[test]
fn flatten_drops_the_nesting() {
    let diags = check("flatten", r"
class ContractFlatten
  extend T::Sig
  sig { params(x: T::Array[T::Array[Integer]]).returns(T::Array[Integer]) }
  def flat(x)
    x.flatten
  end
  # The element survives flatten: `first` is an Integer, so `abs` resolves.
  sig { params(x: T::Array[T::Array[Integer]]).returns(Integer) }
  def head(x)
    x.flatten.first.abs
  end
  # Type arguments are erased for contracts, so the control is a category.
  sig { params(x: T::Array[T::Array[Integer]]).returns(String) }
  def wrong(x)
    x.flatten
  end
end
", None);
    assert_eq!(codes(&diags), ["E0109"], "{diags:?}");
    assert!(diags[0].message.contains("got Array[Integer]"), "{diags:?}");
}

#[test]
fn integer_clamp_with_float_bounds_is_unproven() {
    let diags = check("clamp", r"
class ContractClamp
  extend T::Sig
  sig { params(x: Integer).returns(Float) }
  def bounded(x)
    x.clamp(0.5, 2.5)
  end
  sig { params(x: Integer).returns(String) }
  def wrong(x)
    x.clamp(1, 5)
  end
end
", None);
    assert_eq!(codes(&diags), ["E0109"], "{diags:?}");
    assert!(diags[0].message.contains("got Integer"), "{diags:?}");
}

// -- an ivar is proven only when every writer is visible --

#[test]
fn ivar_with_an_attribute_writer_is_unproven() {
    let source = r#"
class ContractIvarWriter
  extend T::Sig
  attr_writer :name
  attr_accessor :mode
  def initialize
    @name = nil
    @mode = :auto
  end
  sig { returns(String) }
  def shout
    raise "unset" if @name.nil?
    @name.upcase
  end
  sig { returns(String) }
  def label
    @mode
  end
end
class ContractIvarReader
  extend T::Sig
  def initialize
    @mode = :auto
  end
  sig { returns(String) }
  def label
    @mode
  end
end
"#;
    let diags = check("ivar-writer", source, None);
    // The class with no writer path keeps its proof.
    assert_eq!(codes(&diags), ["E0109"], "{diags:?}");
    assert!(diags[0].message.contains("got Symbol"), "{diags:?}");
    assert!(diags[0].start > source.find("class ContractIvarReader").unwrap(), "{diags:?}");
}

#[test]
fn ivar_written_by_reflection_is_unproven() {
    let diags = check("ivar-reflection", r"
class ContractIvarReflected
  extend T::Sig
  def initialize
    @count = nil
  end
  sig { returns(Integer) }
  def total
    raise ArgumentError if @count.nil?
    @count.abs
  end
end
ContractIvarReflected.new.instance_variable_set(:@count, 3)
class ContractIvarEvaled
  extend T::Sig
  def initialize
    @size = nil
  end
  sig { returns(Integer) }
  def total
    @size.abs
  end
end
ContractIvarEvaled.new.instance_eval { @size = 3 }
", None);
    assert!(diags.is_empty(), "{diags:?}");
}

#[test]
fn ivar_written_elsewhere_in_the_family_is_unproven() {
    let diags = check("ivar-family", r"
class ContractIvarThing
end
class ContractIvarBase
  def load
    @value = compute
    @cache ||= compute
  end
  def compute
    3
  end
end
class ContractIvarChild < ContractIvarBase
  extend T::Sig
  def initialize
    @value = nil
    @cache = nil
    @thing = ContractIvarThing.new
  end
  def refill
    @cache ||= compute
  end
  sig { returns(Integer) }
  def value
    raise if @value.nil?
    @value.abs
  end
  sig { returns(Integer) }
  def cache
    raise if @cache.nil?
    @cache.abs
  end
  def poke
    @thing.absent_thing_method
  end
end
", None);
    // `@thing` has one visible writer and no other path: still proven.
    assert_eq!(codes(&diags), ["E0101"], "{diags:?}");
    assert!(diags[0].message.contains("absent_thing_method"), "{diags:?}");
}

#[test]
fn ivar_compound_writes_in_the_same_class_are_unproven() {
    let diags = check("ivar-compound", r"
class ContractIvarMemo
  extend T::Sig
  def initialize
    @memo = nil
    @pair = nil
  end
  def fill
    @memo ||= 3
    @pair, @rest = [1, 2]
  end
  sig { returns(Integer) }
  def memo
    raise if @memo.nil?
    @memo.abs
  end
  sig { returns(Integer) }
  def pair
    raise if @pair.nil?
    @pair.abs
  end
end
", None);
    assert!(diags.is_empty(), "{diags:?}");
}

// -- self in a module is an instance of an unknown includer --

#[test]
fn module_self_is_never_proof_against_an_includer() {
    let diags = check("module-self", r"
module ContractGreets
  extend T::Sig
  sig { returns(ContractPerson) }
  def me
    self
  end
  sig { returns(String) }
  def text
    self
  end
  def register
    ContractRegistry.new.store(self)
  end
end
class ContractPerson
  include ContractGreets
end
class ContractRegistry
  extend T::Sig
  sig { params(person: ContractPerson).void }
  def store(person); end
  sig { returns(ContractPerson) }
  def me
    self
  end
end
", None);
    // A class's own `self` is still proven.
    assert_eq!(codes(&diags), ["E0109"], "{diags:?}");
    assert!(diags[0].message.contains("got ContractRegistry"), "{diags:?}");
}

// -- every redefinition path takes a written contract off the method --

/// Several source files, one project: a redefinition written in another
/// file reaches the class only through a merge-time pass.
fn check_files(name: &str, sources: &[&str]) -> Vec<Diagnostic> {
    let db = Db::default();
    let root = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("sorbet-contract-{name}"));
    let files: Vec<SourceFile> = sources
        .iter()
        .enumerate()
        .map(|(i, text)| SourceFile::new(&db, root.join(format!("source{i}.rb")), (*text).to_owned()))
        .collect();
    ProjectFiles::new(&db, files.clone());
    ClosedWorld::new(&db, true);
    files.iter().flat_map(|f| check_file(&db, *f).clone()).collect()
}

const PATCHED_TARGET: &str = r#"
class ContractPatchSink
  extend T::Sig
  sig { params(text: String).void }
  def self.take(text); end
end
class ContractPatchTarget
  extend T::Sig
  sig { params(value: Integer).returns(Integer) }
  def self.echo(value)
    value
  end
end
ContractPatchTarget.echo("text")
ContractPatchSink.take(ContractPatchTarget.echo(1))
"#;

/// An out-of-line `class << X` body redefines `X.echo`: the written
/// signature no longer governs the method that runs. The control without
/// the patch keeps this a contract rather than a blind spot.
#[test]
fn singleton_patch_redefinition_takes_the_contract_off() {
    let patch = r"
class << ContractPatchTarget
  def echo(value)
    value.to_s
  end
end
";
    let diags = check_files("singleton-patch", &[PATCHED_TARGET, patch]);
    assert!(contract_codes(&diags).is_empty(), "{diags:?}");

    let control = check_files("singleton-patch-control", &[PATCHED_TARGET]);
    assert_eq!(contract_codes(&control), ["E0103", "E0103"], "control must accuse without the patch: {control:?}");
}

/// A module's `self.extended(base)` hook redefines the extender's method
/// when `extend` runs, after the signed `def` was written.
#[test]
fn extended_hook_redefinition_takes_the_contract_off() {
    const TARGET: &str = r#"
class ContractHookTarget
  extend T::Sig
  sig { params(value: Integer).returns(Integer) }
  def echo(value)
    value
  end
  EXTENSION
end
ContractHookTarget.new.echo("text")
"#;
    let hook = r"
module ContractHookInstaller
  def self.extended(base)
    base.define_method(:echo) { |value| value.to_s }
  end
end
";
    let diags = check_files("extended-hook", &[&TARGET.replace("EXTENSION", "extend ContractHookInstaller"), hook]);
    assert!(contract_codes(&diags).is_empty(), "{diags:?}");

    let control = check_files("extended-hook-control", &[&TARGET.replace("EXTENSION", ""), hook]);
    assert_eq!(contract_codes(&control), ["E0103"], "control must accuse without the extend: {control:?}");
}

/// A block reopening in another file redefines the method: like any other
/// duplicate definition, neither signature is proven to be the one that runs.
#[test]
fn class_eval_redefinition_takes_the_contract_off() {
    const TARGET: &str = r#"
class ContractEvalTarget
  extend T::Sig
  sig { params(value: Integer).returns(Integer) }
  def echo(value)
    value
  end
end
ContractEvalTarget.new.echo("text")
"#;
    let reopen = r"
ContractEvalTarget.class_eval do
  def echo(value)
    value.to_s
  end
end
";
    let diags = check_files("class-eval", &[TARGET, reopen]);
    assert!(contract_codes(&diags).is_empty(), "{diags:?}");
    let reopened = r"
class ContractEvalTarget
  alias_method :echo, :to_s
end
";
    let diags = check_files("alias-reopen", &[TARGET, reopened]);
    assert!(contract_codes(&diags).is_empty(), "{diags:?}");
}

/// A descendant that mixes in a module answering the name dispatches to
/// that module, never to the signed parent body.
#[test]
fn descendant_mixin_override_keeps_the_parent_contract_off_the_call() {
    const PARENT: &str = r#"
class ContractMixinParent
  extend T::Sig
  sig { params(value: Integer).returns(Integer) }
  def echo(value)
    value
  end
end
module ContractMixinText
  def echo(value)
    value.to_s
  end
end
class ContractMixinChild < ContractMixinParent
  MIXIN
end
class ContractMixinUser
  extend T::Sig
  sig { params(parent: ContractMixinParent).void }
  def use(parent)
    parent.echo("text")
  end
end
"#;
    let diags = check_files("descendant-mixin", &[&PARENT.replace("MIXIN", "include ContractMixinText")]);
    assert!(contract_codes(&diags).is_empty(), "{diags:?}");
    let diags = check_files("descendant-prepend", &[&PARENT.replace("MIXIN", "prepend ContractMixinText")]);
    assert!(contract_codes(&diags).is_empty(), "{diags:?}");

    let control = check_files("descendant-mixin-control", &[&PARENT.replace("MIXIN", "")]);
    assert_eq!(contract_codes(&control), ["E0103"], "control must accuse without the mixin: {control:?}");
}

/// A module's signed method runs only where no class between an includer
/// and the module redefines it. An includer's subclass that does is a
/// dispatch target the module's contract cannot speak for.
#[test]
fn module_contract_yields_to_an_includer_family_override() {
    const CONCERN: &str = r#"
module ContractConcern
  extend T::Sig
  sig { params(value: Integer).returns(Integer) }
  def echo(value)
    value
  end
end
class ContractConcernHost
  include ContractConcern
end
class ContractConcernChild < ContractConcernHost
  OVERRIDE
end
class ContractConcernUser
  extend T::Sig
  sig { params(host: ContractConcernHost).void }
  def use(host)
    host.echo("text")
  end
end
"#;
    let overridden = CONCERN.replace("OVERRIDE", "def echo(value)\n    value.to_s\n  end");
    let diags = check_files("module-family", &[&overridden]);
    assert!(contract_codes(&diags).is_empty(), "{diags:?}");

    let control = check_files("module-family-control", &[&CONCERN.replace("OVERRIDE", "")]);
    assert_eq!(contract_codes(&control), ["E0103"], "control must accuse without the override: {control:?}");
}

/// A module's `included` hook, in either spelling, redefines the
/// includer's method when `include` runs, after the signed `def`.
#[test]
fn included_hook_redefinition_takes_the_contract_off() {
    const TARGET: &str = r#"
class ContractIncludedTarget
  extend T::Sig
  sig { params(value: Integer).returns(Integer) }
  def echo(value)
    value
  end
  INCLUSION
end
ContractIncludedTarget.new.echo("text")
"#;
    let hook = r"
module ContractIncludedHook
  def self.included(base)
    base.define_method(:echo) { |value| value.to_s }
  end
end
";
    let concern = r"
module ContractIncludedConcern
  extend ActiveSupport::Concern
  included do
    def echo(value)
      value.to_s
    end
  end
end
";
    let diags = check_files("included-hook", &[&TARGET.replace("INCLUSION", "include ContractIncludedHook"), hook]);
    assert!(contract_codes(&diags).is_empty(), "{diags:?}");
    let diags = check_files("included-concern", &[&TARGET.replace("INCLUSION", "include ContractIncludedConcern"), concern]);
    assert!(contract_codes(&diags).is_empty(), "{diags:?}");
    // The block still runs when an earlier macro opened the module first.
    let macro_first = concern.replace("  included do", "  contract_setting :flag\n  included do");
    let diags = check_files("included-macro-first", &[&TARGET.replace("INCLUSION", "include ContractIncludedConcern"), &macro_first]);
    assert!(contract_codes(&diags).is_empty(), "{diags:?}");

    let control = check_files("included-hook-control", &[&TARGET.replace("INCLUSION", ""), hook, concern]);
    assert_eq!(contract_codes(&control), ["E0103"], "control must accuse without the include: {control:?}");
}

// -- cost: contract work is per definition, never per call --

/// A base class, `members` subclasses in their own files, and twelve calls
/// per subclass to the base's two methods, checked with a `sorbet/rbi`
/// directory present (the RBI declares only an unrelated class). Returns
/// the contract work the whole check did on this thread.
fn family_work(name: &str, members: usize, signed: bool) -> itaruby_semantic::check::ContractWork {
    let db = Db::default();
    let root = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("sorbet-contract-{name}"));
    let dir = root.join("sorbet/rbi");
    std::fs::create_dir_all(&dir).unwrap();
    let rbi = dir.join("unrelated.rbi");
    std::fs::write(&rbi, "class ContractUnrelated\n  sig { returns(Integer) }\n  def value; end\nend\n").unwrap();
    RbiProject::new(&db, itaruby_semantic::rbi::build_rbi_index(&[rbi]).constants);
    let sig = |text: &str| if signed { format!("  sig {{ {text} }}\n") } else { String::new() };
    let base = format!(
        "class ContractFamilyBase\n  extend T::Sig\n{}  def helper\n    1\n  end\n{}  def other(value)\n    value\n  end\nend\n",
        sig("returns(Integer)"),
        sig("params(value: Integer).returns(Integer)"),
    );
    let mut files = vec![SourceFile::new(&db, root.join("base.rb"), base)];
    for i in 0..members {
        let calls = "    a = helper\n    b = other(a)\n    c = ContractFamilyBase.new.helper\n".repeat(4);
        let text = format!("class ContractFamilyMember{i} < ContractFamilyBase\n  def run\n{calls}    a\n  end\nend\n");
        files.push(SourceFile::new(&db, root.join(format!("member{i}.rb")), text));
    }
    ProjectFiles::new(&db, files.clone());
    ClosedWorld::new(&db, true);
    let before = itaruby_semantic::check::contract_work();
    for file in &files {
        check_file(&db, *file);
    }
    let after = itaruby_semantic::check::contract_work();
    itaruby_semantic::check::ContractWork {
        family_visits: after.family_visits - before.family_visits,
        contract_resolutions: after.contract_resolutions - before.contract_resolutions,
        return_probes: after.return_probes - before.return_probes,
    }
}

/// Measured before this bound existed: a 1500-member family checked in
/// 26.3s instead of 0.56s once `sorbet/rbi` existed, because every call to
/// an unsigned method walked the whole family twice. Each file now
/// resolves a definition's contract once, walks the family only for a
/// definition that has a contract, and reads the return memo before
/// asking for a contract at all.
#[test]
fn contract_work_is_per_definition_not_per_call() {
    const MEMBERS: u64 = 60;
    let files = MEMBERS + 1;
    let unsigned = family_work("family-unsigned", 60, false);
    assert_eq!(unsigned.family_visits, 0, "no contract, no family walk: {unsigned:?}");
    // Per file: the base's two methods and the member's own `run`
    // (182 measured; one contract per call would be over 900).
    assert!(unsigned.contract_resolutions <= 4 * files, "{unsigned:?}");
    // Per file: three return keys, member#helper, member#other and
    // base#helper (180 measured; one probe per call would be 732).
    assert!(unsigned.return_probes <= 4 * files, "{unsigned:?}");

    let signed = family_work("family-signed", 60, true);
    assert!(signed.contract_resolutions <= 4 * files, "{signed:?}");
    // One walk of the family per signed definition per file (7320
    // measured; one walk per call would be ten times that).
    assert!(signed.family_visits <= 3 * MEMBERS * files, "{signed:?}");
}

/// Include-time code in a subclass may redefine the inherited method on
/// that subclass, through a body no harvest reads.
#[test]
fn include_time_code_in_a_descendant_keeps_the_parent_contract_off_the_call() {
    const FAMILY: &str = r#"
class ContractHookParent
  extend T::Sig
  sig { params(value: Integer).returns(Integer) }
  def echo(value)
    value
  end
end
class ContractHookChild < ContractHookParent
  INCLUSION
end
class ContractHookUser
  extend T::Sig
  sig { params(parent: ContractHookParent).void }
  def use(parent)
    parent.echo("text")
  end
end
"#;
    let hook = r#"
module ContractOpaqueHook
  def self.included(base)
    base.class_eval("def echo(value) = value.to_s")
  end
end
"#;
    let diags = check_files("descendant-include-time", &[&FAMILY.replace("INCLUSION", "include ContractOpaqueHook"), hook]);
    assert!(contract_codes(&diags).is_empty(), "{diags:?}");

    let control = check_files("descendant-include-time-control", &[&FAMILY.replace("INCLUSION", ""), hook]);
    assert_eq!(contract_codes(&control), ["E0103"], "control must accuse without the include: {control:?}");
}

/// sorbet-runtime checks that a value IS an Array or a Hash, never what it
/// holds, so a contract can only hold a value to its collection category.
/// Symbol-keyed literals under `T::Hash[String, ...]` were measured as false
/// E0103/E0109 on correct code (corpus-c, 2026-09-22). The control is the
/// shape the same audit proved a true positive: a Hash returned where the
/// sig promises a String, which sorbet-runtime rejects on every call.
#[test]
fn collection_type_arguments_are_erased_but_the_category_still_accuses() {
    let source = r#"
class ContractErasedGenerics
  extend T::Sig
  sig { returns(T::Hash[String, T.untyped]) }
  def payload
    { user: 1 }
  end
  sig { returns(T::Hash[Symbol, T.untyped]) }
  def request
    { "user" => 1 }
  end
  sig { returns(T::Array[Integer]) }
  def ids
    ["a"]
  end
  sig { params(data: T::Hash[String, T.untyped]).void }
  def update(data); end
  sig { returns(String) }
  def lookup
    { plants: "plants" }
  end
end
ContractErasedGenerics.new.update({ meta: { "k" => 1 } })
ContractErasedGenerics.new.update("not a hash")
"#;
    let diags = check("erased-generics", source, None);
    assert_eq!(contract_codes(&diags), ["E0109", "E0103"], "{diags:?}");
    assert_eq!(&source[diags[0].start..diags[0].end], "lookup", "{diags:?}");
    assert!(diags[1].message.contains("data"), "{diags:?}");
}

/// The keyword loop that binds Sorbet params by name must still WALK every
/// element it cannot bind. A string or constant key, and a `**splat`, used to
/// reach `infer_expr` as a bare assoc node that no arm reads, so every
/// diagnostic inside them vanished (measured 2026-09-22 against main: three
/// E0104 and one E0101 lost, and 4 public-corpus lines gone with them).
#[test]
fn unbindable_keyword_elements_are_still_checked() {
    let diags = check("keyword-walk", r"
class ContractKeywordWalk
  def self.real; end
end
class ContractKeywordTaker
  def take(*args, **opts); end
end
ContractKeywordTaker.new.take('str' => NoSuchConstB)
ContractKeywordTaker.new.take(NoSuchConstC => 1)
ContractKeywordTaker.new.take(**NoSuchConstF)
ContractKeywordTaker.new.take('str' => ContractKeywordWalk.nonexistent_class_method)
ContractKeywordTaker.new.take(sym: NoSuchConstS)
", None);
    let found: Vec<(&str, &str)> = diags.iter()
        .map(|d| (d.code, d.message.as_str()))
        .collect();
    for (code, needle) in [("E0104", "NoSuchConstB"), ("E0104", "NoSuchConstC"), ("E0104", "NoSuchConstF"),
                           ("E0101", "nonexistent_class_method"), ("E0104", "NoSuchConstS")] {
        assert!(found.iter().any(|(c, m)| *c == code && m.contains(needle)), "missing {code} {needle}: {diags:?}");
    }
    assert_eq!(diags.len(), 5, "{diags:?}");
}

/// A flow-derived union is the set of values the body MIGHT hold, and this
/// checker does not narrow through `case`/`when`, `is_a?`, `===`, a guard
/// return or a `raise`, nor through a block, loop or rescue that rewrote a
/// local. So one member that mismatches is not proof; every method below
/// runs under sorbet-runtime. A union is accused only when NO member can be
/// the declared type — see the control test that follows.
#[test]
fn union_with_one_compatible_member_stays_silent() {
    let source = r#"
class ContractUnionShape; end
class ContractUnionOther; end
class ContractUnionMembers
  extend T::Sig
  sig { params(label: String).returns(String) }
  def take(label)
    label
  end
  sig { params(k: T.any(Integer, String)).returns(String) }
  def case_when(k)
    case k
    when Integer then k.to_s
    else k
    end
  end
  sig { params(k: T.any(Integer, String)).returns(Integer) }
  def guard_return(k)
    return k.size unless k.is_a?(Integer)
    k
  end
  sig { params(k: T.any(Integer, String)).returns(String) }
  def guard_raise(k)
    raise ArgumentError unless k.is_a?(String)
    k
  end
  sig { params(k: T.any(Integer, String)).returns(Integer) }
  def or_return(k)
    k.is_a?(Integer) or return 0
    k
  end
  sig { params(k: T.any(Integer, String)).returns(String) }
  def case_equality(k)
    return k.to_s unless String === k
    k
  end
  sig { params(k: T.any(ContractUnionShape, ContractUnionOther)).returns(ContractUnionShape) }
  def project_kind_of(k)
    return ContractUnionShape.new unless k.kind_of?(ContractUnionShape)
    k
  end
  sig { params(k: T.any(ContractUnionShape, ContractUnionOther)).returns(ContractUnionShape) }
  def project_instance_of(k)
    return ContractUnionShape.new unless k.instance_of?(ContractUnionShape)
    k
  end
  sig { params(k: T.any(Integer, String)).returns(String) }
  def guarded_argument(k)
    return take(k) if k.is_a?(String) && k.size > 0
    "none"
  end
  sig { params(name: String, n: Integer).returns(String) }
  def mixed_first(name, n)
    [name, n].first
  end
  sig { params(name: String, n: Integer).returns(Integer) }
  def mixed_last(name, n)
    [name, n].last
  end
  sig { params(items: T::Array[Integer]).returns(String) }
  def block_widened(items)
    x = 1
    items.each { |i| x = "s#{i}" }
    take(x)
  end
  sig { params(n: Integer).returns(String) }
  def while_widened(n)
    x = 1
    while n > 0
      x = "s"
      n -= 1
    end
    take(x)
  end
  sig { params(items: T::Array[Integer]).returns(String) }
  def for_widened(items)
    x = 1
    for i in items
      x = "s#{i}"
    end
    take(x)
  end
  sig { returns(String) }
  def rescue_widened
    x = 1
    begin
      x = Integer("s").to_s
    rescue StandardError
      x = "t"
    end
    take(x)
  end
  sig { params(s: String).returns(T::Boolean) }
  def and_predicate(s)
    s && s.empty?
  end
  sig { params(k: T.any(Integer, String), flag: T.untyped).returns(String) }
  def explicit_union_return(k, flag)
    return k if flag
    "none"
  end
end
ContractUnionMembers.new.take([1, "a"].first)
"#;
    let diags = check("union-members", source, None);
    assert!(contract_codes(&diags).is_empty(), "{:?}", contract_names(source, &diags));
}

/// The controls that keep the union rule a contract: a union whose EVERY
/// member mismatches still accuses — in a tail, an explicit `return` and an
/// argument — and so does a single mismatched type.
#[test]
fn union_with_no_compatible_member_still_accuses() {
    let source = r#"
class ContractUnionMismatch
  extend T::Sig
  sig { params(label: String).returns(String) }
  def take(label)
    label
  end
  sig { params(k: T.any(Integer, Float)).returns(String) }
  def tail_union(k)
    k
  end
  sig { params(k: T.any(Integer, Float), flag: T.untyped).returns(String) }
  def explicit_union(k, flag)
    return k if flag
    "ok"
  end
  sig { params(k: T.any(Integer, Float)).returns(String) }
  def argument_union(k)
    take(k)
  end
  sig { params(k: Integer).returns(String) }
  def single(k)
    k
  end
end
"#;
    let diags = check("union-mismatch", source, None);
    assert_eq!(contract_names(source, &diags), ["tail_union", "explicit_union", "k", "single"], "{diags:?}");
}


// -- nominal compatibility: a class proves NOT to be a core value only from a complete ancestry --

/// A project subclass of a core class IS that core value under
/// sorbet-runtime (`SafeStr.new("x").is_a?(String)`), and so is a subclass
/// of a project reopen of it or of a class some `.rbi` declares (a gem
/// class the project reopens without restating its superclass). An
/// `Instance` is held to a core scalar or collection type only when its
/// ancestry is complete and names nothing that is, or may be, that core
/// type — see the control test that follows.
#[test]
fn core_subclass_instances_satisfy_core_contracts() {
    let source = r#"
class ContractSafeStr < String
end
class ContractParams < Hash
end
class ContractRows < Array
end
class Hash
  def contract_nominal_touch; self; end
end
class ContractReopenParams < Hash
end
class ContractGemBuffer
  def contract_gem_touch; self; end
end
class ContractGemChild < ContractGemBuffer
end
class ContractCoreSubclasses
  extend T::Sig
  sig { params(text: String, data: T::Hash[Symbol, T.untyped], rows: T::Array[Integer]).void }
  def take(text, data, rows); end
  sig { returns(String) }
  def text
    ContractSafeStr.new("x")
  end
  sig { returns(T::Hash[Symbol, T.untyped]) }
  def params
    ContractParams.new
  end
  sig { returns(T::Array[Integer]) }
  def rows
    ContractRows.new
  end
  sig { returns(T.nilable(String)) }
  def maybe_text
    ContractSafeStr.new("y")
  end
  sig { returns(T::Hash[String, Integer]) }
  def reopened
    ContractReopenParams.new
  end
  sig { returns(String) }
  def gem_buffer
    ContractGemBuffer.new
  end
  sig { returns(String) }
  def gem_child
    ContractGemChild.new
  end
end
ContractCoreSubclasses.new.take(ContractSafeStr.new("a"), ContractParams.new, ContractRows.new)
ContractCoreSubclasses.new.take(ContractGemChild.new, ContractReopenParams.new, ContractRows.new)
"#;
    let diags = check("core-subclasses", source, Some(r"
class ContractGemBuffer < String
end
"));
    assert!(contract_codes(&diags).is_empty(), "{:?}", contract_names(source, &diags));
}

/// The controls that keep the ancestry rule a contract: a plain project
/// class, and a project subclass chain, whose complete ancestry names no
/// core type are still accused against `String`, `Hash` and `Array`, in a
/// return and in an argument.
#[test]
fn closed_project_ancestry_still_accuses_core_contracts() {
    let source = r"
class ContractPlainValue
end
class ContractPlainChild < ContractPlainValue
end
class ContractClosedAncestry
  extend T::Sig
  sig { params(text: String).void }
  def take(text); end
  sig { returns(String) }
  def text
    ContractPlainValue.new
  end
  sig { returns(T::Hash[Symbol, T.untyped]) }
  def data
    ContractPlainChild.new
  end
  sig { returns(T::Array[Integer]) }
  def rows
    ContractPlainValue.new
  end
end
ContractClosedAncestry.new.take(ContractPlainChild.new)
";
    let diags = check("closed-ancestry", source, None);
    assert_eq!(
        contract_names(source, &diags),
        ["text", "data", "rows", "ContractPlainChild.new"],
        "{diags:?}"
    );
}

/// Any class can gain a module at runtime by reflection the index never
/// sees as an ancestor edge (`Late.include(M)`, `Late.send(:include, M)`,
/// `Late.class_eval { include M }`, a class method that calls `include`),
/// and `extend` makes a class object itself an instance of the module. So
/// a module-typed contract never accuses anything.
#[test]
fn module_typed_contracts_never_accuse() {
    let source = r#"
module ContractPlugin
end
class ContractLateInclude
end
ContractLateInclude.include(ContractPlugin)
class ContractLateSend
end
ContractLateSend.send(:include, ContractPlugin)
class ContractLateEval
end
ContractLateEval.class_eval { include ContractPlugin }
class ContractLateHook
  def self.plug
    include ContractPlugin
  end
end
ContractLateHook.plug
class ContractLateExtend
end
ContractLateExtend.extend(ContractPlugin)
class ContractPluginUser
  extend T::Sig
  sig { params(plugin: ContractPlugin).void }
  def take(plugin); end
  sig { returns(ContractPlugin) }
  def included
    ContractLateInclude.new
  end
  sig { returns(ContractPlugin) }
  def sent
    ContractLateSend.new
  end
  sig { returns(ContractPlugin) }
  def evaluated
    ContractLateEval.new
  end
  sig { returns(ContractPlugin) }
  def hooked
    ContractLateHook.new
  end
  sig { returns(ContractPlugin) }
  def extended
    ContractLateExtend
  end
  sig { returns(ContractPlugin) }
  def text
    "a string with the module mixed in by reflection"
  end
end
ContractPluginUser.new.take(ContractLateInclude.new)
ContractPluginUser.new.take(ContractLateHook.new)
ContractPluginUser.new.take(1)
"#;
    let diags = check("module-contracts", source, None);
    assert!(contract_codes(&diags).is_empty(), "{:?}", contract_names(source, &diags));
}

/// The control for the module rule: a CLASS-typed contract is still held
/// nominally — an unrelated project class, a core value and a class object
/// are accused, in a return and in an argument.
#[test]
fn class_typed_contracts_still_accuse_unrelated_values() {
    let source = r#"
class ContractClassTarget
end
class ContractClassStranger
end
class ContractClassUser
  extend T::Sig
  sig { params(target: ContractClassTarget).void }
  def take(target); end
  sig { returns(ContractClassTarget) }
  def stranger
    ContractClassStranger.new
  end
  sig { returns(ContractClassTarget) }
  def text
    "text"
  end
end
ContractClassUser.new.take(ContractClassStranger.new)
"#;
    let diags = check("class-contracts", source, None);
    assert_eq!(contract_names(source, &diags), ["stranger", "text", "ContractClassStranger.new"], "{diags:?}");
}

// -- redefinition: every path that redefines or shadows a signed method --

/// A signed class method and a signed instance method, each called once
/// with a String. `BODY` goes inside the class body.
const SCALED: &str = r#"
class ContractScaleCfg
  extend T::Sig
  sig { params(x: Integer).returns(Integer) }
  def self.scale(x)
    x
  end
  sig { params(x: Integer).returns(Integer) }
  def grow(x)
    x
  end
  BODY
end
ContractScaleCfg.scale("text")
ContractScaleCfg.new.grow("text")
"#;

/// Each answers one of the names with a String-taking body.
const STRING_SCALE: &str = r"
module ContractStringScale
  def scale(x)
    x.to_s
  end
end
module ContractStringGrow
  def grow(x)
    x.to_s
  end
end
";

fn scaled(body: &str) -> String {
    SCALED.replace("BODY", body)
}

/// The accused call sites of a `SCALED` project, by receiver method.
fn scaled_accusations(diags: &[Diagnostic], sources: &[&str]) -> Vec<String> {
    let calls = sources[0];
    diags
        .iter()
        .filter(|d| matches!(d.code, "E0103" | "E0109"))
        .map(|d| {
            let line_start = calls[..d.start].rfind('\n').map_or(0, |i| i + 1);
            let line_end = calls[d.start..].find('\n').map_or(calls.len(), |i| d.start + i);
            calls[line_start..line_end].trim().to_owned()
        })
        .collect()
}

/// `def Const.m` written anywhere — top level, another class body, a
/// method body — redefines `Const`'s singleton `m`: the signed `def
/// self.m` is no longer proven to be what runs. The instance track keeps
/// its contract.
#[test]
fn def_on_a_constant_redefines_its_singleton_contract() {
    let base = scaled("");
    let redefs = [
        "def ContractScaleCfg.scale(x)\n  x.to_s\nend\n",
        "class ContractScaleOther\n  def ContractScaleCfg.scale(x)\n    x.to_s\n  end\nend\n",
        "module ContractScaleBoot\n  def self.boot\n    def ContractScaleCfg.scale(x)\n      x.to_s\n    end\n  end\nend\n",
    ];
    for (i, redef) in redefs.iter().enumerate() {
        let sources = [base.as_str(), *redef];
        let diags = check_files(&format!("def-on-const-{i}"), &sources);
        assert_eq!(scaled_accusations(&diags, &sources), ["ContractScaleCfg.new.grow(\"text\")"], "{redef}: {diags:?}");
    }

    let sources = [base.as_str(), "class ContractScaleOther; end\ndef ContractScaleOther.scale(x)\n  x.to_s\nend\n"];
    let control = check_files("def-on-const-control", &sources);
    assert_eq!(
        scaled_accusations(&control, &sources),
        ["ContractScaleCfg.scale(\"text\")", "ContractScaleCfg.new.grow(\"text\")"],
        "control must accuse when the def lands on another constant: {control:?}"
    );
    // `def X.grow` lands on the class object, never on its instances.
    let sources = [base.as_str(), "def ContractScaleCfg.grow(x)\n  x.to_s\nend\n"];
    let control = check_files("def-on-const-instance-control", &sources);
    assert_eq!(
        scaled_accusations(&control, &sources),
        ["ContractScaleCfg.scale(\"text\")", "ContractScaleCfg.new.grow(\"text\")"],
        "a class-object def must keep the instance contract of the same name: {control:?}"
    );
    // `def Integer.+` is a class method: `100 + "R$"` still raises
    // `TypeError` under MRI, so the operand check still accuses it.
    let diags = check("def-on-core", "def Integer.+(other)\n  other\nend\nprice = 100\nprice + \"R$\"\n", None);
    assert!(diags.iter().any(|d| d.code == "E0108"), "{diags:?}");
}

/// A prepend lands in FRONT of the class it targets: a module prepended
/// to the singleton class — `class << self; prepend M; end` in the body,
/// `X.singleton_class.prepend(M)` from outside, `class << X` out of line —
/// shadows the signed class method it answers, also from a method body. A
/// module whose methods are not fully known takes every contract on that
/// track off.
#[test]
fn singleton_prepend_takes_the_shadowed_contract_off() {
    // The last shape's module answers no `scale` itself: its `prepended`
    // hook defines one on the singleton class it lands on.
    let shapes: [(&str, &str); 6] = [
        ("class << self\n    prepend ContractStringScale\n  end", ""),
        ("", "ContractScaleCfg.singleton_class.prepend(ContractStringScale)\n"),
        (
            "",
            "module ContractScaleInstall\n  def self.install\n    ContractScaleCfg.singleton_class.prepend(ContractStringScale)\n  end\nend\nContractScaleInstall.install\n",
        ),
        ("", "class << ContractScaleCfg\n  prepend ContractStringScale\nend\n"),
        ("class << self\n    prepend ContractScaleUnknown::Patch\n  end", ""),
        (
            "class << self\n    prepend ContractScaleHooked\n  end",
            "module ContractScaleHooked\n  def self.prepended(base)\n    base.define_method(:scale) { |x| x.to_s }\n  end\nend\n",
        ),
    ];
    for (i, (body, outside)) in shapes.iter().enumerate() {
        let target = scaled(body);
        let extra = format!("{STRING_SCALE}{outside}");
        let sources = [target.as_str(), extra.as_str()];
        let diags = check_files(&format!("singleton-prepend-{i}"), &sources);
        assert_eq!(scaled_accusations(&diags, &sources), ["ContractScaleCfg.new.grow(\"text\")"], "{body}{outside}: {diags:?}");
    }

    // A fully known module that does not answer `scale` shadows nothing.
    let target = scaled("class << self\n    prepend ContractScaleQuiet\n  end");
    let quiet = "module ContractScaleQuiet\n  def quiet; end\nend\n";
    let sources = [target.as_str(), quiet];
    let control = check_files("singleton-prepend-control", &sources);
    assert_eq!(
        scaled_accusations(&control, &sources),
        ["ContractScaleCfg.scale(\"text\")", "ContractScaleCfg.new.grow(\"text\")"],
        "control must accuse when the prepended module shadows nothing: {control:?}"
    );
}

/// An instance-track prepend, in the body or from outside, shadows the
/// signed instance method it answers; the class method keeps its contract.
#[test]
fn instance_prepend_takes_the_shadowed_contract_off() {
    let shapes: [(&str, &str); 3] = [
        ("prepend ContractStringGrow", ""),
        ("", "ContractScaleCfg.prepend(ContractStringGrow)\n"),
        ("prepend ContractScaleUnknown::Patch", ""),
    ];
    for (i, (body, outside)) in shapes.iter().enumerate() {
        let target = scaled(body);
        let extra = format!("{STRING_SCALE}{outside}");
        let sources = [target.as_str(), extra.as_str()];
        let diags = check_files(&format!("instance-prepend-{i}"), &sources);
        assert_eq!(scaled_accusations(&diags, &sources), ["ContractScaleCfg.scale(\"text\")"], "{body}{outside}: {diags:?}");
    }
}

/// An `include`/`extend` written OUTSIDE the class runs the module's
/// `included`/`extended` hook on it exactly as one in the body does: the
/// hook may redefine or prepend over anything, so every contract of the
/// class comes off. A hookless module mixed in from outside keeps them.
#[test]
fn external_mixin_hook_takes_the_contract_off() {
    let hooks = [
        (
            "ContractScaleCfg.extend(ContractScaleHook)\n",
            "module ContractScaleHook\n  def self.extended(base)\n    base.define_singleton_method(:scale) { |x| x.to_s }\n  end\nend\n",
        ),
        (
            "ContractScaleCfg.extend(ContractScaleHook)\n",
            "module ContractScaleHook\n  def self.extended(base)\n    base.singleton_class.prepend(ContractStringScale)\n  end\nend\n",
        ),
        (
            "ContractScaleCfg.include(ContractScaleHook)\n",
            "module ContractScaleHook\n  def self.included(base)\n    base.prepend(ContractStringGrow)\n  end\nend\n",
        ),
    ];
    for (i, (call, hook)) in hooks.iter().enumerate() {
        let target = scaled("");
        let extra = format!("{STRING_SCALE}{hook}{call}");
        let sources = [target.as_str(), extra.as_str()];
        let diags = check_files(&format!("external-hook-{i}"), &sources);
        assert!(scaled_accusations(&diags, &sources).is_empty(), "{hook}{call}: {diags:?}");
    }

    // The in-body spelling stays silent too.
    let target = scaled("extend ContractScaleHook");
    let extra = format!("{STRING_SCALE}{}", hooks[1].1);
    let sources = [target.as_str(), extra.as_str()];
    let diags = check_files("external-hook-in-body", &sources);
    assert!(scaled_accusations(&diags, &sources).is_empty(), "{diags:?}");

    let target = scaled("");
    let plain = "module ContractScalePlain\n  def plain; end\nend\nContractScaleCfg.include(ContractScalePlain)\nContractScaleCfg.extend(ContractScalePlain)\n";
    let sources = [target.as_str(), plain];
    let control = check_files("external-hook-control", &sources);
    assert_eq!(
        scaled_accusations(&control, &sources),
        ["ContractScaleCfg.scale(\"text\")", "ContractScaleCfg.new.grow(\"text\")"],
        "control must accuse when the external mixin has no hook: {control:?}"
    );
}

/// Whatever redefines a subclass from outside its body — a hooked mixin, a
/// `define_method`, a prepend of a module nobody can read — is a dispatch
/// target the parent's contract cannot speak for.
#[test]
fn outside_redefinition_of_a_subclass_keeps_the_parent_contract_off_the_call() {
    const FAMILY: &str = r#"
class ContractScaleParent
  extend T::Sig
  sig { params(x: Integer).returns(Integer) }
  def grow(x)
    x
  end
end
class ContractScaleChild < ContractScaleParent
  CHILD
end
class ContractScaleUser
  extend T::Sig
  sig { params(parent: ContractScaleParent).void }
  def use(parent)
    parent.grow("text")
  end
end
"#;
    let hook = "module ContractScaleHook\n  def self.included(base)\n    base.define_method(:grow) { |x| x.to_s }\n  end\nend\n";
    let shapes = [
        ("", format!("{hook}ContractScaleChild.include(ContractScaleHook)\n")),
        ("", "ContractScaleChild.define_method(:grow) { |x| x.to_s }\n".to_owned()),
        ("prepend ContractScaleUnknown::Patch", String::new()),
    ];
    for (i, (child, outside)) in shapes.iter().enumerate() {
        let family = FAMILY.replace("CHILD", child);
        let diags = check_files(&format!("outside-family-{i}"), &[&family, outside]);
        assert!(contract_codes(&diags).is_empty(), "{child}{outside}: {diags:?}");
    }

    let control = check_files("outside-family-control", &[&FAMILY.replace("CHILD", ""), hook]);
    assert_eq!(contract_codes(&control), ["E0103"], "control must accuse without the redefinition: {control:?}");
}
