//! CHARACTERIZATION OF A KNOWN GAP — these tests pin CURRENT behavior, not
//! desired behavior. Two certain `NoMethodError`s go unreported:
//! a typo on an `extend`ed module's class method, and a typo on the class
//! method a Rails concern injects via
//! `def self.included(base); base.extend(ClassMethods); end`.
//!
//! Why the gap is still open (measured 2026-09-17, do not re-litigate
//! without re-measuring): the `Ty::Class` `MethodLookup::NotFound` arm in
//! `check.rs` is silent on purpose. An attempt to report that residue was
//! built and measured against the public corpora:
//!
//!   * bare residue report ...... rails +703, mastodon +36, discourse +4109
//!   * narrowed to explicit receivers and to names within edit distance 2
//!     of a class method the index can actually see (the "it is a typo"
//!     signal) ................... rails +216, mastodon +0, discourse +12
//!
//! and every sample inspected in the remainder was a FALSE positive on a
//! method that really exists: `SecureRandom.uuid` and `Kernel.rand`
//! (stdlib module-function surfaces this checker has no inventory for),
//! `ActiveRecord::Marshalling.format_version`,
//! `ActiveSupport::JSON::Encoding.json_encoder` and
//! `DiscourseWorkflows.node_registration_ready=` (`mattr_accessor` /
//! `cattr_accessor`, which `index.rs` does not model at all — zero
//! occurrences), and `RequireProfiler.stats` (`class << self` accessors).
//!
//! A class object's singleton surface is supplied by populations no project
//! index here contains, so "absent from what I can see" is not evidence of
//! absence (AGENTS.md, binding) and reporting the residue violates
//! invariant #1. Closing the gap needs those populations indexed FIRST:
//! (1) `mattr_accessor`/`cattr_accessor`, (2) `class << self` `attr_*`,
//! (3) stdlib module-function inventories. Then the residue can be
//! re-measured against all three private corpora plus the public ones.
//!
//! When someone closes it, these tests fail — that is the point. Flip them
//! to assert the diagnostic, and re-run `scripts/public-gate.sh` and the
//! corpus gates before believing it.
//!
//! Fixtures in `testdata/singleton_lookup/`.

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/singleton_lookup");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap();
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, path.into(), text.clone());
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    let li = itaruby_semantic::LineIndex::new(&text);
    itaruby_semantic::check_file(&db, file)
        .iter()
        .map(|d| {
            let (l, c) = li.line_col(&text, d.start);
            format!("{}:{}:{} {}", l + 1, c + 1, d.code, d.message)
        })
        .collect()
}

/// GAP: `Page.find_by_slog` against `extend Findable` is a certain
/// `NoMethodError` and is not reported. MRI raises on this exact file.
#[test]
fn extend_provided_class_method_typo_is_a_known_gap_currently_silent() {
    let diags = check_fixture("extend_typo_accuses.rb");
    assert!(
        diags.is_empty(),
        "known gap changed: the extend-typo now reports {diags:?} — if this is a deliberate \
         fix, re-run scripts/public-gate.sh (the last attempt added 216 false positives to \
         rails) and flip this test"
    );
}

/// GAP: the Rails concern shape, same story.
#[test]
fn included_hook_class_method_typo_is_a_known_gap_currently_silent() {
    let diags = check_fixture("included_hook_typo_accuses.rb");
    assert!(
        diags.is_empty(),
        "known gap changed: the concern-typo now reports {diags:?} — re-run the public and \
         corpus gates before flipping this test"
    );
}

/// The silence side is correct and must stay silent whatever happens to the
/// gap above: the extended module really supplies the name.
#[test]
fn extend_provided_class_method_resolves_silently() {
    let diags = check_fixture("extend_correct_silent.rb");
    assert!(diags.is_empty(), "expected silence, got: {diags:?}");
}

/// Same, for the name a concern really injects — softened on the method
/// NAME by `soften_not_found`'s dynamic-mixin population.
#[test]
fn included_hook_injected_class_method_stays_silent() {
    let diags = check_fixture("included_hook_correct_silent.rb");
    assert!(diags.is_empty(), "expected silence, got: {diags:?}");
}

/// Invariant #1, and the reason the arm is silent: `name`, `ancestors`,
/// `superclass`, `const_get`, `class_eval` are real methods of every class
/// object and live in no project index. `soften_not_found`'s
/// `kernel_object_singleton_method` check is what keeps them quiet —
/// mutating it to `false` makes all seven of these false-positive
/// (measured).
#[test]
fn class_object_builtin_surface_never_accuses() {
    let diags = check_fixture("builtin_surface_silent.rb");
    assert!(diags.is_empty(), "invariant #1: expected silence, got: {diags:?}");
}
