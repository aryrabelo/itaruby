//! Bead ita-mv5: `unk_project_ret`'s 6 `ret_*` cause sub-buckets — why the
//! CALLEE's own inferred return died on `Ty::Unknown`, for the call sites
//! `tests/call_stats.rs::origin_project_ret` already proves land in
//! `unk_project_ret`. Inline sources in their OWN project, same shape as
//! `call_stats.rs`'s `stats_of` helper.

use itaruby_semantic::{call_stats, CallStats, Db, ProjectFiles, SourceFile};

fn stats_of(text: &str) -> CallStats {
    let db = Db::default();
    let file = SourceFile::new(&db, "inline/ret_cause.rb".into(), text.to_string());
    ProjectFiles::new(&db, vec![file]);
    call_stats(&db, file)
}

/// Every `ret_*` field sums to `unk_project_ret` by construction
/// (`Checker::tally_unknown_receiver`'s `ProjectRet` arm increments both
/// in the same match).
fn assert_sums(s: &CallStats) {
    assert_eq!(
        s.ret_param + s.ret_ivar + s.ret_const + s.ret_explicit + s.ret_other + s.ret_unresolved,
        s.unk_project_ret,
        "ret_* sub-buckets sum to unk_project_ret: {s:?}"
    );
}

/// `Thing#passthru`'s tail statement is a bare read of its own untyped
/// parameter `x` — the exact shape `tests/call_stats.rs::origin_project_ret`
/// already proves lands in `unk_project_ret`. `unknown_origin` classifies
/// that tail node as `UnkOrigin::Param` (structural: `x` is in
/// `Checker::method_params`, checked before any `!silent`-gated
/// bookkeeping — survives `method_return`'s silent re-walk of `passthru`'s
/// body untouched), and `ret_cause_of` maps `Param` straight onto
/// `RetCause::Param`.
#[test]
fn ret_param() {
    let s = stats_of(
        "\
class Thing
  def passthru(x)
    x
  end
end

Thing.new.passthru(1).oops
",
    );
    assert_eq!(s.unk_project_ret, 1, ".oops chains off passthru's Unknown return: {s:?}");
    assert_eq!(s.ret_param, 1, "passthru's tail is its own untyped param x: {s:?}");
    assert_sums(&s);
}

/// Block-parameter variant proving `UnkOrigin::BlockParam` folds into
/// `RetCause::Param` (`ret_cause_of`'s `Param | BlockParam => Param` arm).
/// `passthru`'s tail is `z`, a plain outer local last written INSIDE the
/// `.each` block from its own block parameter `item`. `Checker::method_return`
/// walks `passthru`'s body TWICE for this one fixture: once for real,
/// during `call_stats`'s own non-`silent` primary scan of every `def` in
/// the file (bead ita-au5's existing walk, `check.rs` ~line 793), and
/// once more, `silent`, when `.passthru` is actually resolved below. The
/// FIRST (non-`silent`) walk hits `z = item`'s `LocalVariableWriteNode`
/// arm with `!self.silent` true: it classifies `item` as `BlockParam`
/// (`self.block_params` is live mid-block) and records it in
/// `Checker::local_origin["z"]`. `local_origin`, like `unknown_call_src`,
/// is never reset between `check_method_body` calls — the SECOND
/// (`silent`) walk's OWN attempt to overwrite `local_origin["z"]` is
/// itself `!self.silent`-gated and skipped, so the correct `BlockParam`
/// entry from the first walk is exactly what `unknown_origin` reads back
/// for the tail `z`. `ret_cause_of` then folds it into `RetCause::Param`
/// — the fold the doc comment on `RetCause` names: "a binding the callee
/// received from outside and never refined" describes an explicit param
/// and a block param alike.
#[test]
fn ret_param_via_block_param() {
    let s = stats_of(
        "\
class Thing
  def passthru
    z = nil
    [1].each do |item|
      z = item
    end
    z
  end
end

Thing.new.passthru.oops
",
    );
    assert_eq!(s.unk_project_ret, 1, ".oops chains off passthru's Unknown return: {s:?}");
    assert_eq!(s.ret_param, 1, "z's origin traces to item, a block param — folds into ret_param: {s:?}");
    assert_sums(&s);
}

/// `Thing#passthru`'s tail statement is a bare read of `@thing`, an ivar
/// that `ivar_ty` collapsed to `Unknown` (assigned from `initialize`'s own
/// untyped parameter). `unknown_origin`'s ivar check is structural (no
/// `!silent` gating), so it survives `method_return`'s silent re-walk.
#[test]
fn ret_ivar() {
    let s = stats_of(
        "\
class Thing
  def initialize(x)
    @thing = x
  end

  def passthru
    @thing
  end
end

Thing.new.passthru.oops
",
    );
    assert_eq!(s.unk_project_ret, 1, ".oops chains off passthru's Unknown return: {s:?}");
    assert_eq!(s.ret_ivar, 1, "passthru's tail is @thing, collapsed to Unknown: {s:?}");
    assert_sums(&s);
}

/// `Thing#passthru`'s tail statement is a bare read of `UNKNOWN_THING`,
/// which never resolves against the (empty) project index. Same
/// structural, silent-survives classification as `ret_ivar`.
#[test]
fn ret_const() {
    let s = stats_of(
        "\
class Thing
  def passthru
    UNKNOWN_THING
  end
end

Thing.new.passthru.oops
",
    );
    assert_eq!(s.unk_project_ret, 1, ".oops chains off passthru's Unknown return: {s:?}");
    assert_eq!(s.ret_const, 1, "passthru's tail is the unresolved UNKNOWN_THING: {s:?}");
    assert_sums(&s);
}

/// `Thing#mystery`'s TAIL statement (`1`) is a known type (`Ty::Int`) —
/// `check_method_body`'s `last_is_unknownish` is false — but an earlier
/// `return x if x` folds an explicit `Ty::Unknown` (x is mystery's own
/// untyped param) into the method's overall return via `Ty::union`'s
/// "Unknown absorbs" rule. Per the contract this is `RetCause::Explicit`
/// regardless of what made the EXPLICIT return's own value Unknown.
#[test]
fn ret_explicit() {
    let s = stats_of(
        "\
class Thing
  def mystery(x)
    return x if x
    1
  end
end

Thing.new.mystery(1).oops
",
    );
    assert_eq!(s.unk_project_ret, 1, ".oops chains off mystery's Unknown return: {s:?}");
    assert_eq!(s.ret_explicit, 1, "tail is known (1); an explicit return folded Unknown in: {s:?}");
    assert_sums(&s);
}

/// `Thing#mystery`'s tail statement `x.foo.bar` is a call CHAIN: `x` is
/// mystery's own untyped param, so `x.foo` is itself an unknown-receiver
/// call, and `.bar` chains off THAT — the exact shape
/// `tests/call_stats.rs::origin_dead_chain` classifies as `UnkOrigin::Chain`.
/// `mystery`'s own primary (non-`silent`) scan — every `def` in the
/// censused file gets one, independent of whether it's ever called —
/// already recorded that in `Checker::unknown_call_src`, so `.bar`'s tail
/// classification (reached later, inside `method_return`'s silent
/// re-walk of THIS SAME method) reads it back correctly as `Chain`.
/// `ret_cause_of` has no `Chain` variant of its own — it folds straight
/// to `RetCause::Other`, same as the TRUE ceiling
/// `Checker::check_method_body`'s doc comment names (a call chain whose
/// callee was NEVER primary-scanned at all — cross-file, or reached only
/// via a nested silent walk) would also produce. `ret_param` above proves
/// the bucket that actually matters (bead ita-djm's target) is exact
/// either way: `method_params` populates from `def.parameters()` on
/// every walk, silent or not.
#[test]
fn ret_other() {
    let s = stats_of(
        "\
class Thing
  def mystery(x)
    x.foo.bar
  end
end

Thing.new.mystery(1).oops
",
    );
    assert_eq!(s.unk_project_ret, 1, ".oops chains off mystery's Unknown return: {s:?}");
    assert_eq!(
        s.ret_other, 1,
        "tail is a call chain (Chain), which ret_cause_of folds into Other: {s:?}"
    );
    assert_sums(&s);
}

/// `Widget` declares `method_missing` so it is open: `.nonexistent_method`
/// is `MethodLookup::Inconclusive` — there is no callee body to walk at
/// all, so `note_unknown_origin` passes `cause: None`. `.oops` chains off
/// that Unknown with no cause to attribute — `ret_unresolved`, not any
/// `RetCause` variant (there is no callee return to classify).
#[test]
fn ret_unresolved() {
    let s = stats_of(
        "\
class Widget
  def method_missing(name, *args)
    nil
  end
end

Widget.new.nonexistent_method.oops
",
    );
    assert_eq!(s.unk_project_ret, 1, ".oops chains off an Inconclusive lookup: {s:?}");
    assert_eq!(s.ret_unresolved, 1, "no callee to blame a parameter/ivar/const on: {s:?}");
    assert_sums(&s);
}

/// Regression for the memo path: `Thing#passthru` is chained off at TWO
/// distinct call sites. `Checker::method_return`'s `return_memo` is keyed
/// by `(class, name, singleton)` only — no argument values — so the
/// SECOND `.passthru(2).oops` hits the memo, not a fresh
/// `check_method_body` walk. If the memo cached only the return TYPE
/// (not the cause), the second site's `self.last_ret_cause` would be
/// stale/`None` and this call site would land in `ret_unresolved` instead
/// of `ret_param` — `return_cause_memo`, read in lockstep with
/// `return_memo` at the exact same key, is what keeps it `ret_param`.
#[test]
fn ret_cause_survives_the_return_memo() {
    let s = stats_of(
        "\
class Thing
  def passthru(x)
    x
  end
end

Thing.new.passthru(1).oops
Thing.new.passthru(2).oops
",
    );
    assert_eq!(s.unk_project_ret, 2, "two independent .oops call sites: {s:?}");
    assert_eq!(s.ret_param, 2, "the memo-hit 2nd call site keeps passthru's real cause: {s:?}");
    assert_sums(&s);
}
