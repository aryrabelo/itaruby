//! Call-site coverage census (bead ita-38f): every `check_call` outcome
//! lands in exactly one bucket, and the census counts each call site in
//! the text exactly once. Inline sources in their OWN project (same
//! shape as `core_conclusive`'s inline fixtures): an open class or an
//! E0101 fixture in `testdata/` would leak into the merged gate-c run.

use itaruby_semantic::{call_stats, CallStats, Db, ProjectFiles, SourceFile};

fn stats_of(text: &str) -> CallStats {
    let db = Db::default();
    let file = SourceFile::new(&db, "inline/call_stats.rb".into(), text.to_string());
    ProjectFiles::new(&db, vec![file]);
    call_stats(&db, file)
}

/// One fixture forcing each of the 5 buckets with exact counts.
/// `Box.new` resolves (lookup `initialize` `NotFound` still concludes);
/// `.size` resolves against Box; `.nmae` concludes absence → diagnosed
/// (the E0101 itself is discarded here). `Widget` declares
/// `method_missing` so it is open: BOTH `Widget.new` and
/// `.nonexistent_method` are inconclusive — receiver class known,
/// lookup cannot conclude — and the chained `.frog` runs on the Unknown
/// that arm returned → `unknown_receiver`. `"hi".upcase` resolves against
/// the core allowlist → core.
#[test]
fn every_bucket_forced_once() {
    let s = stats_of(
        "\
class Widget
  def method_missing(name, *args)
    nil
  end
end

class Box
  def size
    1
  end
end

Box.new.size
Box.new.nmae
Widget.new.nonexistent_method.frog
\"hi\".upcase
",
    );
    assert_eq!(s.resolved, 1, ".size only — Box.new has no visible initialize, a silent-inconclusive site since ita-gjb: {s:?}");
    assert_eq!(s.core, 1, "\"hi\".upcase: {s:?}");
    assert_eq!(s.diagnosed, 1, ".nmae on closed Box: {s:?}");
    assert_eq!(s.inconclusive, 4, "Box.new x2 (no visible initialize, ita-gjb) + Widget.new + .nonexistent_method (method_missing opens the class): {s:?}");
    assert_eq!(s.unknown_receiver, 1, ".frog chained off an inconclusive Unknown: {s:?}");
    assert_eq!(
        s.unk_project_ret, 1,
        ".frog's receiver (`.nonexistent_method`) is an Inconclusive project call → project ret: {s:?}"
    );
    assert_eq!(s.total(), 8, "every call site in the text counted exactly once: {s:?}");
    assert_eq!(s.blind(), 5, "blind = inconclusive + unknown_receiver: {s:?}");
}

/// The `silent` gate: reading `@calc` in `total`/`label` forces
/// `ivar_ty` → `walk_class_ivars(Invoice)`, a silent re-walk of EVERY
/// instance method; `method_return` likewise re-walks found bodies.
/// Ungated, each call site below would be tallied once per walk and
/// `total()` would be a multiple (2x here) of the text's call-site
/// count. The census must equal the text: exactly 3 call sites —
/// `Calculator.new` (inconclusive — no visible `initialize`, ita-gjb), `@calc.sum` (resolved),
/// `@calc.tag` (diagnosed).
#[test]
fn silent_rewalks_do_not_double_count() {
    let s = stats_of(
        "\
class Calculator
  def sum
    10
  end
end

class Invoice
  def initialize
    @calc = Calculator.new
  end

  def total
    @calc.sum
  end

  def label
    @calc.tag
  end
end
",
    );
    assert_eq!(s.total(), 3, "must equal the text's call-site count, not a multiple: {s:?}");
    assert_eq!(s.resolved, 1, "@calc.sum only — Calculator.new has no visible initialize (ita-gjb): {s:?}");
    assert_eq!(s.diagnosed, 1, "@calc.tag — Calculator has no tag: {s:?}");
    assert_eq!(s.blind(), 1, "Calculator.new is the one blind (silent-inconclusive) site: {s:?}");
}

/// Bead ita-au5: `unknown_receiver`'s 8 origin sub-buckets, one call site
/// forcing each. `x` is `bar`'s own required parameter (untyped, so
/// `Ty::Unknown`) — `x.mystery`'s receiver classifies as `Param`.
#[test]
fn origin_param() {
    let s = stats_of(
        "\
class Foo
  def bar(x)
    x.mystery
  end
end
",
    );
    assert_eq!(s.unknown_receiver, 1, "x.mystery: {s:?}");
    assert_eq!(s.unk_param, 1, "x is bar's own unrefined parameter: {s:?}");
    assert_eq!(
        s.unk_param + s.unk_block_param + s.unk_core_ret + s.unk_project_ret + s.unk_chain
            + s.unk_ivar + s.unk_const + s.unk_other,
        s.unknown_receiver,
        "sub-buckets sum to unknown_receiver: {s:?}"
    );
}

/// `item` is `.each`'s block parameter (always `Ty::Unknown`) —
/// `item.mystery`'s receiver classifies as `BlockParam`. `.each` itself
/// resolves against the core allowlist (`Bucket::Core`).
#[test]
fn origin_block_param() {
    let s = stats_of(
        "\
class Foo
  def each_thing
    [1, 2, 3].each do |item|
      item.mystery
    end
  end
end
",
    );
    assert_eq!(s.core, 1, "[1,2,3].each: {s:?}");
    assert_eq!(s.unknown_receiver, 1, "item.mystery: {s:?}");
    assert_eq!(s.unk_block_param, 1, "item is .each's block param: {s:?}");
}

/// `Hash#[]` is a modeled core method whose return is deliberately
/// `CoreRet::Unknown` (no static key-type tracking) — `{}[:missing]`
/// resolves (`Bucket::Core`) but yields `Ty::Unknown`; `.oops`'s receiver
/// chains off that call and classifies as `CoreRet`.
#[test]
fn origin_core_ret() {
    let s = stats_of("{}[:missing].oops\n");
    assert_eq!(s.core, 1, "{{}}[:missing]: {s:?}");
    assert_eq!(s.unknown_receiver, 1, ".oops: {s:?}");
    assert_eq!(s.unk_core_ret, 1, ".oops chains off Hash#[]'s unmodeled return: {s:?}");
}

/// `Thing#mystery` returns its own untyped parameter unchanged, so its
/// inferred return is `Ty::Unknown` — the lookup itself still succeeds
/// (`Bucket::Resolved`). `.oops`'s receiver chains off `.mystery(1)` and
/// classifies as `ProjectRet`.
#[test]
fn origin_project_ret() {
    let s = stats_of(
        "\
class Thing
  def mystery(x)
    x
  end
end

Thing.new.mystery(1).oops
",
    );
    assert_eq!(s.resolved, 1, ".mystery(1) only — Thing.new has no visible initialize (ita-gjb): {s:?}");
    assert_eq!(s.unknown_receiver, 1, ".oops: {s:?}");
    assert_eq!(s.unk_project_ret, 1, ".oops chains off Thing#mystery's Unknown return: {s:?}");
}

/// `x.mystery` is itself an unknown-receiver call site (`x` a param →
/// `Param`); `.chain` then runs on `x.mystery`'s own `Ty::Unknown`
/// result, so `.chain`'s receiver — a call whose OWN receiver was
/// already unknown — classifies as `Chain` ("dead chain").
#[test]
fn origin_dead_chain() {
    let s = stats_of(
        "\
class Foo
  def bar(x)
    x.mystery.chain
  end
end
",
    );
    assert_eq!(s.unknown_receiver, 2, "x.mystery + .chain: {s:?}");
    assert_eq!(s.unk_param, 1, "x.mystery: {s:?}");
    assert_eq!(s.unk_chain, 1, ".chain runs on x.mystery's already-Unknown result: {s:?}");
}

/// `@thing` is assigned an untyped parameter in `initialize`, so
/// `ivar_ty` collapses it to `Ty::Unknown` — `@thing.oops`'s receiver
/// classifies as `Ivar`.
#[test]
fn origin_ivar() {
    let s = stats_of(
        "\
class Foo
  def initialize(x)
    @thing = x
  end

  def use
    @thing.oops
  end
end
",
    );
    assert_eq!(s.unknown_receiver, 1, "@thing.oops: {s:?}");
    assert_eq!(s.unk_ivar, 1, "@thing collapsed to Unknown via the untyped param x: {s:?}");
}

/// `UNKNOWN_THING` never resolves against the (empty) project index —
/// `infer_const` returns `Ty::Unknown` — so `.oops`'s receiver
/// classifies as `Const`.
#[test]
fn origin_const() {
    let s = stats_of(
        "\
class Foo
  def use
    UNKNOWN_THING.oops
  end
end
",
    );
    assert_eq!(s.unknown_receiver, 1, "UNKNOWN_THING.oops: {s:?}");
    assert_eq!(s.unk_const, 1, "UNKNOWN_THING never resolves: {s:?}");
}

/// A bare top-level call with no receiver at all: `self` at true
/// top-level is itself `Ty::Unknown`, so `oops`'s own receiver is
/// `None` (implicit self) — classifies as `Other`.
#[test]
fn origin_other() {
    let s = stats_of("oops\n");
    assert_eq!(s.unknown_receiver, 1, "oops: {s:?}");
    assert_eq!(s.unk_other, 1, "implicit self at toplevel has no receiver node: {s:?}");
}

/// Bead ita-au5's own silent-gate regression: `Invoice#total` reads
/// `@calc` (forces `ivar_ty` → a silent re-walk of BOTH Invoice methods)
/// then calls `.compute(1)` on it (forces `method_return` → a silent
/// re-walk of `Calculator#compute`'s body, which itself contains the
/// fixture's only real unknown-receiver call site, `x.mystery`).
/// `x.mystery` is walked twice: once for real (`Calculator#compute`'s
/// own direct pass) and once inside each silent re-walk. Un-gated, the
/// sub-buckets would double- or triple-count it; the census must equal
/// the text's real blind call-site count (2 — `x.mystery` plus `Calculator.new`,
/// a silent-inconclusive site since ita-gjb), not a multiple.
#[test]
fn census_sub_buckets_stay_silent_gated() {
    let s = stats_of(
        "\
class Calculator
  def compute(x)
    x.mystery
  end
end

class Invoice
  def initialize
    @calc = Calculator.new
  end

  def total
    @calc.compute(1)
  end
end
",
    );
    assert_eq!(s.total(), 3, "Calculator.new + x.mystery + @calc.compute(1): {s:?}");
    assert_eq!(s.blind(), 2, "x.mystery + Calculator.new (no visible initialize, ita-gjb) are blind, not a multiple of it: {s:?}");
    assert_eq!(s.unknown_receiver, 1, "x.mystery, walked exactly once for real: {s:?}");
    assert_eq!(s.unk_param, 1, "x.mystery's receiver is compute's own param x: {s:?}");
    assert_eq!(
        s.unk_param + s.unk_block_param + s.unk_core_ret + s.unk_project_ret + s.unk_chain
            + s.unk_ivar + s.unk_const + s.unk_other,
        s.unknown_receiver,
        "sub-buckets sum to unknown_receiver, not a multiple: {s:?}"
    );
}
