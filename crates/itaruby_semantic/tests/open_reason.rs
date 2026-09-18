//! Bead ita-anc: attribution for `ancestry open` (`MethodLookup::Inconclusive`
//! from an open ancestor), the single largest blind bucket in `ita check
//! --stats`. `OpenReason` records WHY a fragment was opened, at every
//! `open = true` site in `DefWalker`/`merge_declared_fragment`;
//! `ProjectIndex::inconclusive_reason` walks an ancestor chain and answers
//! the counterfactual that actually matters: would this lookup close if we
//! fixed only project-side opens, or is it blocked by an ancestor we would
//! need real gem knowledge (Tapioca RBI) to close? Inline sources in their
//! own project, same shape as `tests/dynamic_include.rs`'s `index_for`.

use itaruby_semantic::index::{Blocker, OpenReason};
use itaruby_semantic::types::ClassId;
use itaruby_semantic::{Db, ProjectFiles, ProjectIndex, SourceFile};

fn index_of(text: &str) -> ProjectIndex {
    let db = Db::default();
    let file = SourceFile::new(&db, "inline/open_reason.rb".into(), text.to_string());
    ProjectFiles::new(&db, vec![file]);
    itaruby_semantic::project_index(&db).clone()
}

fn class_id(index: &ProjectIndex, path: &str) -> ClassId {
    *index.by_path.get(path).unwrap_or_else(|| panic!("class `{path}` not indexed"))
}

/// A dynamic superclass expression (`< Struct.new(...)`) makes the ancestry
/// itself unknowable — `DefWalker`'s `ClassNode` arm, the `None if
/// class.superclass().is_some()` branch.
#[test]
fn dynamic_superclass() {
    let index = index_of("class DynSuper < Struct.new(:a)\nend\n");
    let id = class_id(&index, "DynSuper");
    assert_eq!(
        index.inconclusive_reason(id, false),
        Some(Blocker::Project(OpenReason::DynamicSuperclass))
    );
}

/// `def method_missing`/`def respond_to_missing?` — every unknown-method
/// send might actually be handled.
#[test]
fn method_missing() {
    let index = index_of(
        "class MissesStuff\n  def method_missing(name, *args)\n  end\nend\n",
    );
    let id = class_id(&index, "MissesStuff");
    assert_eq!(
        index.inconclusive_reason(id, false),
        Some(Blocker::Project(OpenReason::MethodMissing))
    );
}

/// The `raise NotImplementedError` abstract-class text-scan idiom.
#[test]
fn abstract_raise() {
    let index = index_of(
        "class AbstractBase\n  def do_it\n    raise NotImplementedError\n  end\nend\n",
    );
    let id = class_id(&index, "AbstractBase");
    assert_eq!(
        index.inconclusive_reason(id, false),
        Some(Blocker::Project(OpenReason::AbstractRaise))
    );
}

/// `include`/`extend`/`prepend` called on a receiver that isn't implicit or
/// `self` — we don't know which object is actually being mixed into.
#[test]
fn dynamic_mixin_receiver() {
    let index = index_of("class MixinRecv\n  SomeObj.include Mixable\nend\n");
    let id = class_id(&index, "MixinRecv");
    assert_eq!(
        index.inconclusive_reason(id, false),
        Some(Blocker::Project(OpenReason::DynamicMixinReceiver))
    );
}

/// `include`/`extend`/`prepend` with implicit/`self` receiver but a
/// non-constant-path argument.
#[test]
fn dynamic_mixin_arg() {
    let index = index_of("class MixinArg\n  include mixin_for(:x)\nend\n");
    let id = class_id(&index, "MixinArg");
    assert_eq!(
        index.inconclusive_reason(id, false),
        Some(Blocker::Project(OpenReason::DynamicMixinArg))
    );
}

/// `attr_reader`/`attr_writer`/`attr_accessor` with a non-symbol argument.
/// Named for `attr_*` only: real visibility modifiers never open a class
/// (they just recurse into the wrapped `def`), so no other shape reaches
/// this variant.
#[test]
fn dynamic_attr_arg() {
    let index = index_of("class AttrDyn\n  attr_accessor \"count\"\nend\n");
    let id = class_id(&index, "AttrDyn");
    assert_eq!(
        index.inconclusive_reason(id, false),
        Some(Blocker::Project(OpenReason::DynamicAttrArg))
    );
}

/// A block on a class-body call that is not `define_method` — the
/// `ActiveSupport::Concern` idiom. Its own variant (bead ita-o1n) because
/// the fix differs from the DSL catch-all: the block body has to be
/// walked in class scope, not allowlisted away.
#[test]
fn class_body_block() {
    let index = index_of("class Concerned\n  included do\n    scope :active, -> {}\n  end\nend\n");
    let id = class_id(&index, "Concerned");
    assert_eq!(
        index.inconclusive_reason(id, false),
        Some(Blocker::Project(OpenReason::ClassBodyBlock))
    );
}

/// `class << <non-self expr>`: a singleton class on something we cannot
/// resolve. Not a class-body call, so it must not land in the DSL bucket.
#[test]
fn singleton_class_expr() {
    let index = index_of("class SingletonOther\n  class << other_thing\n  end\nend\n");
    let id = class_id(&index, "SingletonOther");
    assert_eq!(
        index.inconclusive_reason(id, false),
        Some(Blocker::Project(OpenReason::SingletonClassExpr))
    );
}

/// `define_method` with a non-literal name (dynamic name, no block form).
#[test]
fn dynamic_define_method() {
    let index = index_of("class DefinesDynamic\n  define_method(compute_name)\nend\n");
    let id = class_id(&index, "DefinesDynamic");
    assert_eq!(
        index.inconclusive_reason(id, false),
        Some(Blocker::Project(OpenReason::DynamicDefineMethod))
    );
}

/// `alias_method` with a non-literal new-name argument.
#[test]
fn dynamic_alias_method() {
    let index =
        index_of("class AliasDyn\n  alias_method compute_new_name, :old_method\nend\n");
    let id = class_id(&index, "AliasDyn");
    assert_eq!(
        index.inconclusive_reason(id, false),
        Some(Blocker::Project(OpenReason::DynamicAliasMethod))
    );
}

/// `class_eval`/`module_eval`/`instance_eval`/`send`/`public_send`/
/// `__send__`/`delegate`/`define_singleton_method` — arbitrary code
/// execution/dispatch, no way to know what it defines.
#[test]
fn eval_or_send() {
    let index = index_of("class SendsStuff\n  send(:some_method)\nend\n");
    let id = class_id(&index, "SendsStuff");
    assert_eq!(
        index.inconclusive_reason(id, false),
        Some(Blocker::Project(OpenReason::EvalOrSend))
    );
}

/// The catch-all `_ =>` arm: any bare class-body call itaruby doesn't
/// specifically model (`has_many`, Dry's `option`, ...). `validates` is
/// deliberately NOT one of them any more — it moved to the
/// `defines_no_method` allowlist, which the next test pins.
#[test]
fn unknown_class_body_call() {
    let index = index_of("class HasDsl\n  has_many :things\nend\n");
    let id = class_id(&index, "HasDsl");
    assert_eq!(
        index.inconclusive_reason(id, false),
        Some(Blocker::Project(OpenReason::UnknownClassBodyCall))
    );
}

/// The other side of the same instrument (bead ita-o1n): a class whose
/// only class-body calls are framework hooks/validators that define
/// nothing stays CLOSED, so its lookups can conclude. Both sides matter —
/// the test above proves a real generator still opens the class, this one
/// proves the allowlist actually closes it. Delete `defines_no_method` and
/// this test fails; allowlist `has_many` and the test above fails.
#[test]
fn allowlisted_framework_calls_leave_class_closed() {
    let index = index_of(
        "class Hooked\n  before_action :authenticate\n  validates :name, presence: true\n  after_commit :notify\n  def run\n  end\nend\n",
    );
    let id = class_id(&index, "Hooked");
    assert_eq!(
        index.inconclusive_reason(id, false),
        None,
        "hooks and validators define no method: nothing here justifies opening the class"
    );
}

/// One generator among allowlisted calls still opens the class — the
/// allowlist is per-call, never per-class, so a single `scope` poisons the
/// whole body exactly as before.
#[test]
fn one_generator_among_hooks_still_opens() {
    let index = index_of(
        "class Mixedish\n  before_action :authenticate\n  scope :active, -> {}\nend\n",
    );
    let id = class_id(&index, "Mixedish");
    assert_eq!(
        index.inconclusive_reason(id, false),
        Some(Blocker::Project(OpenReason::UnknownClassBodyCall))
    );
}

/// `merge_declared_fragment`: `declarations/gems.rbi` force-opens the
/// curated external namespace unconditionally.
#[test]
fn declared_external() {
    // `ActiveRecord::Base` is never defined by this inline project, so
    // `merge_declared_fragment` fills it in from `declarations/gems.rbi`.
    let index = index_of("class Model < ActiveRecord::Base\nend\n");
    let id = class_id(&index, "Model");
    assert_eq!(
        index.inconclusive_reason(id, false),
        Some(Blocker::Declared)
    );
}

/// An ancestor name that resolves NOWHERE — not the project, not
/// `declarations/gems.rbi` — leaves `ancestors()` incomplete. Different
/// blocker from `Declared` because the fix is different: this one needs
/// the NAME to resolve, not methods attached to a name we already have.
#[test]
fn unresolved_ancestor_name() {
    let index = index_of("class Orphan < SomeGemNobodyDeclared::Base\nend\n");
    let id = class_id(&index, "Orphan");
    assert_eq!(
        index.inconclusive_reason(id, false),
        Some(Blocker::Unresolved)
    );
}

/// Precedence, and the reason it is not proximity: `Mixed` inherits from a
/// name we cannot resolve AND includes a `gems.rbi`-declared module. Give
/// every declared entry a real method set and this site is STILL blocked,
/// so the honest counterfactual answer is `Unresolved`, never `Declared`.
#[test]
fn unresolved_beats_declared() {
    let index = index_of(
        "class Mixed < SomeGemNobodyDeclared::Base\n  include ActiveSupport::Concern\nend\n",
    );
    let id = class_id(&index, "Mixed");
    assert_eq!(
        index.inconclusive_reason(id, false),
        Some(Blocker::Unresolved),
        "an unresolvable ancestor survives the fix for every declared-open ancestor"
    );
}

/// A class opened for two reasons (in source order) keeps the first —
/// `DefWalker::open_class`'s first-reason-wins guard.
#[test]
fn two_reasons_keeps_the_first() {
    let index = index_of(
        "class TwoReasons\n  def method_missing(name, *args)\n  end\n  has_many :things\nend\n",
    );
    let id = class_id(&index, "TwoReasons");
    assert_eq!(
        index.inconclusive_reason(id, false),
        Some(Blocker::Project(OpenReason::MethodMissing)),
        "method_missing is written first; the later `has_many` catch-all open must not overwrite it"
    );
}

/// A plain closed class — nothing open, ancestry fully resolved — is not
/// ancestry-blocked at all.
#[test]
fn closed_class_is_none() {
    let index = index_of("class ClosedClass\n  def foo\n  end\nend\n");
    let id = class_id(&index, "ClosedClass");
    assert_eq!(index.inconclusive_reason(id, false), None);
}

/// The counterfactual rule, and the single most important test in this
/// file: `Model` is itself open for a project-side reason (the `has_many`
/// DSL call, unmodeled) AND inherits from `ActiveRecord::Base`, which
/// `declarations/gems.rbi` declares (permanently open, external). Fixing
/// the project-side open alone would NOT close this lookup — the external
/// ancestor still blocks it — so `External` must win over the project-side
/// reason, exactly as `inconclusive_reason`'s doc comment specifies.
#[test]
fn external_ancestor_wins_over_project_side_open() {
    let index = index_of(
        "class Model < ActiveRecord::Base\n  has_many :things\nend\n",
    );
    let id = class_id(&index, "Model");
    assert_eq!(
        index.inconclusive_reason(id, false),
        Some(Blocker::Declared),
        "Model is open for a project-side reason too, but the external \
         ActiveRecord::Base ancestor is what actually blocks the lookup"
    );
}

/// A project-side-only open chain (no external ancestor anywhere) reports
/// `Project`, never `External` — the closable population `ita-anc` exists
/// to measure.
#[test]
fn project_only_open_chain_is_project_blocker() {
    let index = index_of(
        "class ProjectOnly\n  def method_missing(name, *args)\n  end\nend\n",
    );
    let id = class_id(&index, "ProjectOnly");
    assert!(matches!(
        index.inconclusive_reason(id, false),
        Some(Blocker::Project(_))
    ));
}

// -- Bead ita-h6l, mechanism B: `class X ... end` reopening a Ruby
// core/stdlib/gem name must open the fragment (`OpenReason::
// ReopenedExternal`) regardless of what the reopening body writes — the
// real class's ancestry runs through methods this project never modeled.
// MUTANTS THIS BLOCK MUST CATCH:
//   1. `is_known_external_class_path`'s carve-out removed entirely (the
//      `if is_known_external_class_path(&full) { ... }` block deleted from
//      `DefWalker`'s `ClassNode` arm) — every test below flips from
//      `Some(Blocker::Project(ReopenedExternal))` to `None`.
//   2. The carve-out broadened to match ordinary project names (e.g. any
//      capitalized single-segment path, or `is_known_external_class_path`
//      losing its exact-match discipline) — `project_reopening_stays_closed`
//      flips from `None` to `Some(...)`.

/// Source 1: a Ruby core class name (`core::is_known_core_constant`).
/// `Range` is real Rails-reopened territory (`core_ext/range`).
#[test]
fn reopened_core_class_opens() {
    let index = index_of("class Range\n  def overlap?(other)\n  end\nend\n");
    let id = class_id(&index, "Range");
    assert_eq!(
        index.inconclusive_reason(id, false),
        Some(Blocker::Project(OpenReason::ReopenedExternal))
    );
}

/// Source 2: a `declarations/stdlib_constants.txt` entry not covered by
/// `is_known_core_constant` — `ERB` is real Rails-reopened territory
/// (`ActionView`'s `erb/util.rb`).
#[test]
fn reopened_stdlib_class_opens() {
    let index = index_of("class ERB\n  def result_with_hash(hash)\n  end\nend\n");
    let id = class_id(&index, "ERB");
    assert_eq!(
        index.inconclusive_reason(id, false),
        Some(Blocker::Project(OpenReason::ReopenedExternal))
    );
}

/// Source 3: a `declarations/gems.rbi` namespace, matched by its full
/// qualified path — `Mail::Message` is real Rails-reopened territory
/// (`ActionMailer`'s delivery methods).
#[test]
fn reopened_gem_class_opens() {
    let index = index_of("class Mail::Message\n  def deliver_now\n  end\nend\n");
    let id = class_id(&index, "Mail::Message");
    assert_eq!(
        index.inconclusive_reason(id, false),
        Some(Blocker::Project(OpenReason::ReopenedExternal))
    );
}

/// The counter-proof (mutant 2): an ordinary project class whose name
/// matches none of the three sources stays CLOSED — the carve-out must
/// never become a blanket "any reopening is open" rule. Two fragments
/// (real reopening shape) so a broadened mutant that only checks "was
/// this class defined more than once" is also caught.
#[test]
fn project_reopening_stays_closed() {
    let index = index_of(
        "class ReopenH6lOrdinaryModel\n  def known_method\n  end\nend\n\
         class ReopenH6lOrdinaryModel\n  def another_known_method\n  end\nend\n",
    );
    let id = class_id(&index, "ReopenH6lOrdinaryModel");
    assert_eq!(index.inconclusive_reason(id, false), None);
}
