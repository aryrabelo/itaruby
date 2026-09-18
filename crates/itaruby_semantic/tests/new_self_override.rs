//! Bead ita-6bq: `X.new(args)` used to always model `initialize`'s arity,
//! even when `X` (or an ancestor) defines its OWN singleton `new` — the
//! actual dispatch target. A project (or `rails/activesupport`'s
//! `ActiveSupport::Deprecation::DeprecationProxy`, which defines
//! `def self.new(*args, **kwargs, &block)`) that overrides `self.new` to
//! do argument-shuffling before delegating to `allocate`/`initialize`
//! makes `initialize`'s arity a lie about what `.new(...)` really accepts.
//!
//! Fix (see `Checker::infer_expr_inner`'s `Ty::Class(c)` "new" arm in
//! `check.rs`): before falling back to the existing `initialize`-based
//! arity check, look up an own singleton `new` via `ProjectIndex::
//! lookup_singleton` (the SAME ancestry/`extend` walk every other
//! singleton dispatch already uses — deliberately not the `_rbi` variant,
//! since `soften_not_found` treats `new` as a builtin Class/Module method
//! and would mask "no own `self.new`" as `Inconclusive`).
//!   - `Found`: that signature — not `initialize`'s — now governs arity.
//!     No separate "is this shape modelable" gate exists: `check_arity`'s
//!     own `m.arity_unknown || m.rest` guard already no-ops for a
//!     variadic/forwarding self.new (`*args`, `...`), and `check_sig_args`
//!     only ever fires when `m.sig` is `Some` (an RBS `#:` comment), which
//!     a `**kwargs`-only shape leaves untouched either way — exactly the
//!     same reliance on their internal guards every OTHER singleton-method
//!     dispatch in this file already uses (no shape pre-filtering
//!     anywhere else). A prototype `new_sig_is_modelable` gate duplicating
//!     `m.rest`/`m.arity_unknown` here was proven mutation-blind (it never
//!     changed an observable diagnostic — `check_arity`'s own guard
//!     already covered every case it covered — only the census tally
//!     moved) and was removed.
//!   - `NotFound` (ancestry fully closed, no own `self.new` anywhere in
//!     it): falls through UNCHANGED to the existing `initialize`-based
//!     match, including the ita-gjb fail-closed arm (no visible
//!     `initialize` anywhere → silence).
//!   - `Inconclusive` (ancestry can't prove one way or the other — e.g. an
//!     open ancestor): must NOT fall through to the `initialize` match
//!     either — an invisible ancestor `self.new` may still exist, so
//!     `initialize`'s arity proves nothing. Silence (invariant #1), same
//!     `tally_inconclusive`/`tally_ar_base` census treatment as every
//!     other genuinely ancestry-blocked singleton lookup in this file.
//!     Real-world proof: rails/activesupport's actual `DeprecationProxy`
//!     opens itself via a class-body `instance_methods.each { |m|
//!     undef_method m }` block (ita-d0j) — a subclass with its own
//!     `initialize` used to get a fabricated E0102 from that unrelated
//!     arity before this arm existed (see mutant 4 and fixture (e)).
//!
//! Fixtures live under `testdata/new_self_override/` with a `NewSelfOv`
//! class-name prefix — `testdata/` is scanned as one merged project by
//! `ita check testdata/` (gate c), so names must stay globally unique.
//!
//! MUTANTS THIS FILE MUST CATCH:
//!   1. The own-`self.new` lookup skipped entirely (the new `if let
//!      MethodLookup::Found(m, owner) = self.index.lookup_singleton(c,
//!      "new") { ... }` block deleted, falling straight through to the
//!      old `initialize`-only match): `silent_variadic_self_new_produces_
//!      no_diagnostic` flips from empty to a single E0102 — its
//!      `initialize` is a 0-arg decoy that would wrongly govern the 3-arg
//!      call once `self.new` is no longer consulted.
//!   2. Arity still read from `initialize` even once an own `self.new` is
//!      `Found` (e.g. the `check_arity`/`check_sig_args` calls left
//!      pointed at the old `m` from the `initialize` lookup instead of the
//!      new one): `accusing_fixed_arity_self_new_fires` flips from one
//!      E0102 to silence — its `initialize(a, b)` has the SAME arity as
//!      the 2-argument call, so reading arity from `initialize` instead of
//!      `self.new(x)` would wrongly miss the mismatch.
//!   3. The `NotFound` fallback removed or narrowed (e.g. the
//!      pre-existing `initialize`-based match's own arms altered):
//!      `control_no_override_wrong_arity_still_warns` or
//!      `control_no_override_no_initialize_silent` moves off its expected
//!      outcome — proving classes untouched by this bead keep
//!      byte-identical behavior.
//!   4. `MethodLookup::Inconclusive` from the self.new lookup wrongly
//!      falls through to the `initialize`-based match instead of
//!      returning silence immediately (e.g. the `match` collapsed back to
//!      an `if let Found(...)`, or the `Inconclusive` arm's `return`
//!      dropped): `open_ancestry_self_new_produces_no_diagnostic` flips
//!      from empty to a single E0102 — the open parent's `self.new` stays
//!      invisible, and the child's own 3-arg `initialize` would then
//!      wrongly govern the 2-argument call. This is the exact real-world
//!      `DeprecationProxy` false positive this bead's Found-only version
//!      still produced.
//!
//! NOT a mutant this file needs to catch (verified, not just asserted):
//! any gate re-added on top of `check_arity`/`check_sig_args` that
//! duplicates their own `rest`/`kwrest`/`arity_unknown` handling is
//! provably unobservable — `silent_variadic_self_new_produces_no_
//! diagnostic` stays silent with or without such a gate, because
//! `check_arity`'s `m.rest` guard and `check_sig_args`' `m.sig`-only check
//! already silence it. A gate whose removal changes nothing but a census
//! bucket is dead weight, not a defended invariant.

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/new_self_override");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, path.clone().into(), text);
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    itaruby_semantic::check_file(&db, file)
        .iter()
        .map(|d| {
            format!(
                "{}[{}]: {}",
                if d.severity == itaruby_semantic::Severity::Error { "error" } else { "warning" },
                d.code,
                d.message
            )
        })
        .collect()
}

/// (a) An own `self.new(*args, **kwargs, &block)` (variadic/forwarding,
/// the real `DeprecationProxy` shape) makes the real accepted call shape
/// unmodelable — silence, even though the call passes 3 arguments and the
/// class's own `initialize` (a mismatched 0-arg decoy) would otherwise
/// have fired.
#[test]
fn silent_variadic_self_new_produces_no_diagnostic() {
    let diags = check_fixture("silent_variadic_self_new.rb");
    assert!(diags.is_empty(), "expected silence, got: {diags:?}");
}

/// (b) An own `self.new(x)` (fixed arity 1) governs `.new(...)`'s arity —
/// the 2-argument call fires E0102 against `self.new`'s signature, even
/// though `initialize(a, b)` (arity 2) would have silently matched had it
/// still been the thing being checked. This is the accusing control that
/// proves the arity now comes from `self.new`.
#[test]
fn accusing_fixed_arity_self_new_fires() {
    let diags = check_fixture("accusing_fixed_arity_self_new.rb");
    assert_eq!(diags.len(), 1, "expected exactly one arity diagnostic, got: {diags:?}");
    assert!(
        diags[0].contains("E0102") && diags[0].contains("expects 1 argument, got 2"),
        "expected a self.new-arity mismatch, got: {diags:?}"
    );
}

/// (c) No own `self.new`: current behavior is untouched — arity still
/// comes from `initialize(a)` (arity 1), and the 0-argument call fires
/// E0102 exactly as it did before this bead.
#[test]
fn control_no_override_wrong_arity_still_warns() {
    let diags = check_fixture("control_no_override_wrong_arity_still_warns.rb");
    assert_eq!(diags.len(), 1, "expected exactly one arity diagnostic, got: {diags:?}");
    assert!(
        diags[0].contains("E0102") && diags[0].contains("expects 1 argument, got 0"),
        "expected a real initialize-arity mismatch, got: {diags:?}"
    );
}

/// (d) No own `self.new` AND no visible `initialize` anywhere: the
/// pre-existing ita-gjb fail-closed arm still silences this, unaffected by
/// this bead.
#[test]
fn control_no_override_no_initialize_silent() {
    let diags = check_fixture("control_no_override_no_initialize_silent.rb");
    assert!(diags.is_empty(), "expected silence (ita-gjb control), got: {diags:?}");
}

/// (e) The real rails/activesupport `DeprecationProxy` shape: the parent
/// defines a variadic `self.new` but also opens itself via a class-body
/// `instance_methods.each { }` block (ita-d0j), so `lookup_singleton`
/// returns `Inconclusive`, not `Found` — the walk never reaches a
/// definitive answer. The child's own `initialize(a, b, c)` (arity 3)
/// must NOT govern the 2-argument `.new` call: an invisible ancestor
/// `self.new` may still exist. Silence is the only sound answer.
#[test]
fn open_ancestry_self_new_produces_no_diagnostic() {
    let diags = check_fixture("open_ancestry_silent.rb");
    assert!(diags.is_empty(), "expected silence (open ancestry), got: {diags:?}");
}
