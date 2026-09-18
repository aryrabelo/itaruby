//! Bead ita-uye: conclusiveness of an in-block self-send when the method
//! RECEIVING the block provably never rebinds `self`.
//!
//! Bead ita-4xy made every receiverless call inside a block inconclusive,
//! because a DSL method may `instance_exec` the block against a context
//! object — correct under invariant #1, but it also silenced a real dormant
//! `NameError` inside a plain iterator block (recorded in
//! `scripts/corpus-baseline.txt` as the corpus-a error hash removed on
//! 2026-08-21). Ruby changes `self` in a block through exactly one family of
//! methods (`instance_eval`/`instance_exec`/`class_eval`/`module_eval`/
//! `class_exec`/`module_exec`/`define_method`), so a core method known to
//! merely yield gives lexical `self` with certainty — but only when the
//! RECEIVER's class is known too: proof here is class IDENTITY, not the
//! method name alone. An `Unknown`-receiver arm that trusted the name alone
//! (on the argument that the Enumerable/iteration protocol's published
//! contract forbids rebinding) was built and measured, then removed by
//! review decision: name-only evidence is argument, not proof. See
//! `AGENTS.md` for the measurement this file's regression-lock test defends.
//!
//! Both directions are proven here, because a rule proven on one side only is
//! half a measurement: the iterator block must accuse again, and the DSL shape
//! that motivated ita-4xy must stay silent. The pollution guard needs its own
//! isolated project (a reopened `Array` in `testdata/` would silence Array
//! receivers tree-wide), so those two tests build their sources inline.
//!
//! MUTANTS THIS FILE MUST CATCH:
//!
//!   1. call site `let rebindable = !self.block_keeps_lexical_self(&recv_ty, &name);`
//!      -> `let rebindable = false;`               (softening removed entirely)
//!   2. same site -> `let rebindable = true;`      (bead ita-4xy restored)
//!   3. core arm drops the allowlist term: `core_block_keeps_lexical_self(name) &&`
//!      removed, leaving only `self.core_class_unpolluted(cc)`
//!   4. core arm drops the pollution guard: `let Some(_cc) = ...` and body becomes
//!      `core_block_keeps_lexical_self(name)`
//!   5. `self.rebindable_block_depth += usize::from(rebindable);` -> `=`
//!      (nesting flattened)
//!   6. the guard's fallback `return false` -> `return core_block_keeps_lexical_self(name)`
//!      — i.e. the reverted arm sneaking back in its broad form. This is the
//!      REGRESSION LOCK for this whole slice and must be caught.
//!
//! Two lessons survive from before the revert, both still true: **a silence
//! fixture only pins the arm it actually REACHES** — nothing pins the
//! allowlist itself unless a fixture gives a KNOWN core receiver a
//! rebinding method name (`core_instance_eval_silent.rb`), and nothing pins
//! the project-receiver exclusion unless a fixture gives a resolvable
//! PROJECT receiver an allowlisted name (`project_own_each_silent.rb`) —
//! every other silence fixture here uses an unlisted name and never reaches
//! that arm. And the **structural trap**: while the predicate opens with
//! `let Some(cc) = core_class_of(recv_ty) else { return false }`, a mutant
//! aimed at the expression BELOW that guard cannot reach a non-core
//! receiver at all — it reads as CAUGHT while proving far less than it
//! appears to. Mutants 1 and 2 target the CALL SITE for that reason; mutant
//! 6 targets the guard's own fallback for the same reason.

fn diags_of(sources: &[(&str, &str)]) -> Vec<String> {
    let db = itaruby_semantic::Db::default();
    let files: Vec<_> = sources
        .iter()
        .map(|(name, text)| {
            itaruby_semantic::SourceFile::new(&db, format!("/p/{name}").into(), (*text).to_string())
        })
        .collect();
    itaruby_semantic::ProjectFiles::new(&db, files.clone());
    files
        .iter()
        .flat_map(|f| itaruby_semantic::check_file(&db, *f))
        .map(|d| format!("{} {}", d.code, d.message))
        .collect()
}

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/block_self");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap();
    diags_of(&[(name, &text)])
}

/// The recovered true positive: `Integer#times` and `String#each_char` only
/// yield, so a self-send of a method the class never defines is a real
/// `NameError` and E0101 is conclusive again.
#[test]
fn lexical_iterator_block_accuses() {
    let diags = check_fixture("iteration_block_accuses.rb");
    assert_eq!(diags.len(), 2, "expected 2 diagnostics, got: {diags:?}");
    assert!(
        diags.iter().all(|d| d.contains("E0101")),
        "both must be E0101, got: {diags:?}"
    );
    assert!(
        diags.iter().any(|d| d.contains("blkself_missing_from_times")),
        "the `times` block must accuse, got: {diags:?}"
    );
    assert!(
        diags
            .iter()
            .any(|d| d.contains("blkself_missing_from_each_char")),
        "the `each_char` block must accuse, got: {diags:?}"
    );
}

/// The mandatory counter-proof of the bead: the corpus shape that made
/// ita-4xy soften in the first place. A project method receiving the block
/// is never proven, so the block stays soft.
#[test]
fn instance_exec_dsl_block_stays_silent() {
    let diags = check_fixture("dsl_rebind_silent.rb");
    assert!(
        diags.is_empty(),
        "a block handed to a project method may be instance_exec'd: silence \
         is the only safe answer. Got: {diags:?}"
    );
}

/// "Lexical" means the enclosing scope's `self`, and that scope is itself
/// rebound here. A proven-lexical `times` nested inside a rebound DSL block
/// must not re-enable conclusiveness.
#[test]
fn lexical_block_nested_in_rebound_block_stays_silent() {
    let diags = check_fixture("nested_block_inherits_rebind.rb");
    assert!(
        diags.is_empty(),
        "the enclosing block already made `self` unknowable. Got: {diags:?}"
    );
}

/// The unknown-receiver arm (name-only evidence trusting the
/// Enumerable/iteration protocol's published contract) was built, measured,
/// and removed by review decision — proof here requires class IDENTITY, and
/// an unknown receiver has none. This is the REGRESSION LOCK for mutant 6:
/// if the guard's fallback ever again answers
/// `core_block_keeps_lexical_self(name)` instead of `false`, this test goes
/// from silent to accusing. The accepted cost, paid deliberately: the one
/// read-verified corpus true positive (hash `92479c91c0ae76e2`) that arm
/// recovered goes back to being silent.
#[test]
fn unknown_receiver_with_iteration_method_stays_silent() {
    let diags = check_fixture("unknown_receiver_each_silent.rb");
    assert!(
        diags.is_empty(),
        "an unknown receiver is never conclusive, even with an allowlisted \
         name: proof here requires class identity. Got: {diags:?}"
    );
}

/// The other side: the narrowing costs nothing where the receiver IS known,
/// because there class identity is the proof rather than the name.
#[test]
fn core_receiver_with_yielding_name_still_accuses() {
    let diags = check_fixture("core_tap_accuses.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(
        diags[0].contains("blkself_core_tap_missing"),
        "got: {:?}",
        diags[0]
    );
}

/// The asymmetry, and the counter-example that justifies it: a project class
/// that defines its OWN `each` and `instance_exec`s the block. With an
/// unknown receiver we trust the name; here we can see the callee rebinds, so
/// knowing more must not license concluding more. Pins the `None` arm's
/// exclusion of resolvable project receivers — the only test that does,
/// because every other project receiver here uses an unlisted name and never
/// reaches that arm.
#[test]
fn project_class_with_own_rebinding_each_stays_silent() {
    let diags = check_fixture("project_own_each_silent.rb");
    assert!(
        diags.is_empty(),
        "this `each` instance_execs the block and the method really exists on \
         the context object: accusing would be a false positive. Got: {diags:?}"
    );
}

/// A known core receiver is not enough: the METHOD must be one that only
/// yields. `instance_eval` is the exact opposite, and `self` inside its block
/// is the String — so accusing here would name the wrong class, a real false
/// positive. Pins the allowlist behaviourally: a mutant that treats every
/// core method as lexical is caught by this test alone.
#[test]
fn core_receiver_with_rebinding_method_stays_silent() {
    let diags = check_fixture("core_instance_eval_silent.rb");
    assert!(
        diags.is_empty(),
        "`instance_eval` rebinds self even on a core receiver. Got: {diags:?}"
    );
}

/// The pollution guard, in its own project: the same `[1, 2].each` that would
/// be conclusive goes soft once the project reopens `Array`, because the
/// reopened `each` could `instance_exec` the block.
#[test]
fn reopened_core_class_disqualifies_its_iterators() {
    let reopen = "class Array\n  def blkself_reopened_helper\n    1\n  end\nend\n";
    let user = "class BlkSelfReopenUser\n  def run\n    [1, 2].each { blkself_reopen_missing }\n  end\nend\n";

    let without = diags_of(&[("user.rb", user)]);
    assert_eq!(
        without.len(),
        1,
        "control: with Array untouched, `each` is proven lexical and this \
         accuses. Got: {without:?}"
    );
    assert!(without[0].contains("E0101"), "got: {:?}", without[0]);

    let with = diags_of(&[("reopen.rb", reopen), ("user.rb", user)]);
    assert!(
        with.is_empty(),
        "a reopened `Array#each` may rebind self: the same call must go \
         silent. Got: {with:?}"
    );
}

/// A `method_missing` defined at TOP level pollutes every receiver
/// (`core_mixin`), so nothing is provable anywhere in that project.
#[test]
fn toplevel_method_missing_disqualifies_every_iterator() {
    let user = "class BlkSelfMixinUser\n  def run\n    2.times { blkself_mixin_missing }\n  end\nend\n";
    let control = diags_of(&[("user.rb", user)]);
    assert_eq!(control.len(), 1, "control must accuse, got: {control:?}");

    let polluted = diags_of(&[
        ("mm.rb", "def method_missing(name, *args)\n  nil\nend\n"),
        ("user.rb", user),
    ]);
    assert!(
        polluted.is_empty(),
        "toplevel method_missing sets core_mixin: every core receiver's \
         method table is unknowable. Got: {polluted:?}"
    );
}

/// The rebinding family must never be in the allowlist — that is the whole
/// safety argument, so it is pinned rather than left to inspection.
#[test]
fn rebinding_methods_are_not_allowlisted() {
    for name in [
        "instance_eval",
        "instance_exec",
        "class_eval",
        "class_exec",
        "module_eval",
        "module_exec",
        "define_method",
    ] {
        assert!(
            !itaruby_semantic::core::core_block_keeps_lexical_self(name),
            "`{name}` rebinds self and must never be proven lexical"
        );
    }
    assert!(itaruby_semantic::core::core_block_keeps_lexical_self("each"));
}
