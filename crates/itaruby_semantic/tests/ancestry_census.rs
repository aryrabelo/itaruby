//! Bead ita-anc: `inconclusive`'s 6 `anc_*` ancestry-blocker sub-buckets —
//! WHICH kind of `open`/incomplete ancestor is actually stopping the
//! lookup, and specifically whether it is closable from inside the
//! project (`anc_dsl`/`anc_meta`/`anc_missing`/`anc_other`) or needs real
//! gem knowledge (`anc_declared`) — the counterfactual bead ita-anc's
//! contract exists to measure. Inline sources in their OWN project, same
//! shape as `call_stats.rs`'s `stats_of` helper.

use itaruby_semantic::{call_stats, CallStats, Db, ProjectFiles, SourceFile};

fn stats_of(text: &str) -> CallStats {
    let db = Db::default();
    let file = SourceFile::new(&db, "inline/ancestry_census.rb".into(), text.to_string());
    ProjectFiles::new(&db, vec![file]);
    call_stats(&db, file)
}

/// Every `anc_*` field sums to `inconclusive` by construction
/// (`Checker::tally_inconclusive` tallies `Bucket::Inconclusive` and its
/// `anc_*` breakdown in the same call, for every arm that reaches
/// `Bucket::Inconclusive`).
fn assert_sums(s: &CallStats) {
    assert_eq!(
        s.anc_declared + s.anc_dsl + s.anc_meta + s.anc_missing + s.anc_other + s.anc_na,
        s.inconclusive,
        "anc_* sub-buckets sum to inconclusive: {s:?}"
    );
}

/// `Model` inherits from `ActiveRecord::Base`, a `declarations/gems.rbi`
/// entry: `merge_declared_fragment` force-opens every declared fragment
/// unconditionally (bead ita-3gs), so `ActiveRecord::Base` sits in
/// `Model`'s ancestor chain as a permanently-open, EXTERNAL blocker.
/// `Model` defines its own zero-arg `initialize` (bead ita-k9j, entrega
/// 2: `initialize` is itself real `ActiveRecord::Base` instance API —
/// without a project definition, `Model.new` would now resolve through
/// the curated-inventory escalation instead of staying `Inconclusive`,
/// which would make this fixture measure the wrong thing). So `Model.new`
/// resolves directly against the project's own `initialize`
/// (`Bucket::Resolved`) and returns `Ty::Instance(Model)`; only the
/// chained `.foo` (`Ty::Instance(c)` arm, `foo` in neither AR API set)
/// hits the external ancestor and lands in `anc_declared`.
#[test]
fn anc_declared_declared_gem_ancestor() {
    let s = stats_of(
        "\
class Model < ActiveRecord::Base
  def initialize
  end
end

Model.new.foo
",
    );
    assert_eq!(s.resolved, 1, "Model.new resolves via the project's own initialize: {s:?}");
    assert_eq!(s.inconclusive, 1, "only .foo is blocked by ActiveRecord::Base: {s:?}");
    assert_eq!(s.anc_declared, 1, "declared gem ancestor is an External blocker, not project-side: {s:?}");
    assert_sums(&s);
}

/// `has_many :things` is an unmodeled class-body DSL call: `DefWalker`'s
/// catch-all `_ =>` arm (`index.rs` ~line 576) opens `Model` with
/// `OpenReason::UnknownClassBodyCall`. TWO call sites land in
/// `anc_dsl`: the `has_many :things` statement itself is type-checked as
/// a plain class-body expression (`Checker::scope_stmt`'s fallback arm,
/// self = `Ty::Class(Model)`) — `lookup_singleton(Model, "has_many")`
/// hits the now-open `Model` fragment immediately and is Inconclusive;
/// and `Model.new` (the `new`-with-`Inconclusive` arm) hits the same open
/// fragment resolving `initialize`.
#[test]
fn anc_dsl_unmodeled_class_body_call() {
    let s = stats_of(
        "\
class Model
  has_many :things
end

Model.new
",
    );
    assert_eq!(s.inconclusive, 2, "has_many :things itself + Model.new, both on the open Model fragment: {s:?}");
    assert_eq!(s.anc_dsl, 2, "Model opened by the unmodeled `has_many` DSL call: {s:?}");
    assert_sums(&s);
}

/// `send(:attr_accessor, :x)` with no receiver and no block hits the
/// named `class_eval | module_eval | ... | send | ...` arm
/// (`index.rs` ~line 552-555), opening `Model` with
/// `OpenReason::EvalOrSend`. Same shape as the `anc_dsl` fixture above:
/// the `send(...)` statement's own singleton lookup, plus `Model.new`'s
/// `initialize` lookup, both land on the same open fragment.
#[test]
fn anc_meta_send() {
    let s = stats_of(
        "\
class Model
  send(:attr_accessor, :x)
end

Model.new
",
    );
    assert_eq!(s.inconclusive, 2, "send(...) itself + Model.new, both on the open Model fragment: {s:?}");
    assert_eq!(s.anc_meta, 2, "Model opened by `send`, an EvalOrSend metaprogramming call: {s:?}");
    assert_sums(&s);
}

/// `Model` declares `method_missing`, opening it with
/// `OpenReason::MethodMissing` (`index.rs` ~line 300). Unlike the DSL/meta
/// fixtures, the opening statement is a `def` — `Checker::scope_stmt`'s
/// `DefNode` arm walks its body (`nil`, not a call), so it contributes NO
/// call site of its own. `Model.new` (`new`-with-`Inconclusive`) and the
/// chained `.foo` (`Ty::Instance(c)` arm) are the only two Inconclusive
/// sites, both `anc_missing`.
#[test]
fn anc_missing_method_missing() {
    let s = stats_of(
        "\
class Model
  def method_missing(name, *args)
    nil
  end
end

Model.new.foo
",
    );
    assert_eq!(s.inconclusive, 2, "Model.new + .foo, both blocked by method_missing's open flag: {s:?}");
    assert_eq!(s.anc_missing, 2, "Model opened by defining method_missing: {s:?}");
    assert_sums(&s);
}

/// `1.mystery_method`: a concrete core receiver (`Ty::Int`) with no
/// modeled core method — the "concrete-core-receiver miss" arm, the one
/// `Bucket::Inconclusive` site that is NOT gated on a project `ClassId`
/// at all. With no `ClosedWorld` input wired into this test's `Db`,
/// `Checker::closed_world` is false, so `core_unknown_is_conclusive`
/// short-circuits false and the miss falls to the silent `else` branch
/// (`Bucket::Inconclusive`, never `Diagnosed`). `check_call` passes
/// `tally_inconclusive(None)` here — there is no project ancestor to
/// blame — landing in `anc_na`, never invented into an ancestry cause.
#[test]
fn anc_na_non_project_receiver() {
    let s = stats_of("1.mystery_method\n");
    assert_eq!(s.inconclusive, 1, "the sole call site, an unmodeled Integer method: {s:?}");
    assert_eq!(s.anc_na, 1, "not an ancestry block at all — receiver isn't a project class: {s:?}");
    assert_sums(&s);
}

/// All 5 forced shapes above, mixed in one file, plus `anc_other`'s own
/// ceiling: `Blocker::Project(OpenReason::DeclaredExternal)` is
/// unreachable through `inconclusive_reason` by the index's own contract
/// (`merge_declared_fragment` always sets a declared fragment's blocker
/// to `External`, never `Project(DeclaredExternal)`) — `anc_other` stays
/// 0 here, and there is no fixture that can force it nonzero. The 6-way
/// sum is the invariant this batch exists to guarantee: it can never
/// drift from `inconclusive` because `tally_inconclusive` increments both
/// in the same call, for every arm that reaches `Bucket::Inconclusive`.
#[test]
fn six_way_sum_equals_inconclusive() {
    let s = stats_of(
        "\
class DslModel
  has_many :things
end

class MetaModel
  send(:attr_accessor, :x)
end

class MissingModel
  def method_missing(name, *args)
    nil
  end
end

class ExternalModel < ActiveRecord::Base
  def initialize
  end
end

DslModel.new
MetaModel.new
MissingModel.new.foo
ExternalModel.new.foo
1.mystery_method
",
    );
    assert_eq!(s.anc_dsl, 2, "has_many :things + DslModel.new: {s:?}");
    assert_eq!(s.anc_meta, 2, "send(...) + MetaModel.new: {s:?}");
    assert_eq!(s.anc_missing, 2, "MissingModel.new + .foo: {s:?}");
    assert_eq!(
        s.anc_declared, 1,
        "ExternalModel.new resolves via its own initialize (bead ita-k9j entrega 2); only .foo is blocked by ActiveRecord::Base: {s:?}"
    );
    assert_eq!(s.anc_other, 0, "unreachable: no declared fragment is ever Project(DeclaredExternal): {s:?}");
    assert_eq!(s.anc_na, 1, "1.mystery_method, not an ancestry block: {s:?}");
    assert_eq!(s.inconclusive, 8, "2+2+2+1+1 across the 5 mixed shapes: {s:?}");
    assert_sums(&s);
}
