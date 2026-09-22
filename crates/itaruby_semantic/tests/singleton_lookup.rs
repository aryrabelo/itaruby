//! THE CLASS-OBJECT FLIP, CLOSED 2026-09-21
//!
//! The gap is CLOSED. Two `NoMethodError`s that went unreported before the
//! flip are now diagnostics:
//! * a typo on an `extend`ed module's class method (`Page.find_by_slog`), and
//! * a typo on a class method a Rails concern injects via
//!   `def self.included(base); base.extend(ClassMethods); end`
//!   (`Record.default_scope_nmae`).
//!
//! These tests pin the CLOSED state. The two fixtures now assert E0101
//! emission. The flip landed twelve mechanisms that populate the
//! singleton-surface inventory and reduced the public-corpus residue from
//! 54 records to 8 (every one read at its byte offset and proven to raise
//! in `scripts/public-baseline/README.md`).
//!
//! ## Historical context — why the gap was measured as open
//!
//! Pre-flip (2026-09-17), the `Ty::Class` `MethodLookup::NotFound` arm in
//! `check.rs` was silent on purpose. An attempt to report that residue was
//! built and measured against the public corpora:
//!
//!   * bare residue report ...... rails +703, mastodon +36, discourse +4109
//!   * narrowed to explicit receivers and to names within edit distance 2
//!     of a class method the index can actually see (the "it is a typo"
//!     signal) ................... rails +216, mastodon +0, discourse +12
//!
//! Every sample inspected in the remainder was a FALSE positive on a
//! method that really exists: `SecureRandom.uuid` and `Kernel.rand`
//! (stdlib module-function surfaces this checker has no inventory for),
//! `ActiveRecord::Marshalling.format_version`,
//! `ActiveSupport::JSON::Encoding.json_encoder` and
//! `DiscourseWorkflows.node_registration_ready=` (`mattr_accessor` /
//! `cattr_accessor`, which `index.rs` does not model at all — zero
//! occurrences), and `RequireProfiler.stats` (`class << self` accessors).
//!
//! A class object's singleton surface is supplied by populations no project
//! index contained, so "absent from what I can see" was not evidence of
//! absence (AGENTS.md, binding) and reporting the residue would have violated
//! invariant #1. Closing the gap required those populations indexed FIRST:
//! (1) `mattr_accessor`/`cattr_accessor`, (2) `class << self` `attr_*`,
//! (3) stdlib module-function inventories. The flip landed all three plus nine
//! other beads.
//!
//! ## Measurement history (2026-09-18 audit)
//!
//! Pre-flip residue (re-measured at 588b5ed, after the singleton-track steps
//! through the class-level attribute macros, the mocking-gem softening, and
//! the concern-edge harvest gate). The metric: a lookup reaching `NotFound`
//! has already passed `lookup_singleton` (which returns `Inconclusive` the
//! moment ANY ancestor is open) and `soften_not_found` (kernel/object
//! singleton surface, stdlib inventory, mocking-gem population, dynamic
//! mixins, gem reopenings). Probe built from the exact revision under test:
//!
//!   corpus    | explicit receiver (total)
//!   rails     |  42 (193)
//!   mastodon  |   0 (22)
//!   discourse |  14 (605)
//!   corpus-c  |   2 (1665)
//!
//! The explicit-receiver families (why they stayed open pre-flip):
//!
//! * `ActiveSupport` core extensions on `Module`/`Class` objects (`descendants`,
//!   `module_parent*`, `in?`): 19 of rails' 42 — a gem's own core-extension
//!   surface, which no path-keyed inventory can see (the receiver is ANY
//!   class), a distinct population from the stdlib module-function inventory.
//! * `with`/railtie-DSL/Singleton-pattern sites (`Foo.config`,
//!   `Subscriber.instance`): 10 on rails — defined through gem-internal
//!   class-object machinery (hooks, `class << self` inside gem files the
//!   project never opens).
//! * corpus-c's 2: `self.node_type`/`self.edge_type` inside a
//!   `Class.new(GraphQL::Types::Relay::BaseEdge) do ... end` block — the
//!   runtime receiver is the ANONYMOUS class, while the checker resolves
//!   lexical `self`; a rebindable-block-shaped fix of its own.
//! * discourse's 14: project classes whose class methods are installed by
//!   plugin/`add_to_class`-style machinery from other files, plus two gem
//!   reopens (`DiscourseSubscriptions`).
//! * rails' `RaisesNoMethodError.foobar_method_doesnt_exist`: 1 DELIBERATE
//!   true positive — the fixture exists to raise `NoMethodError`.
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

/// CLOSED 2026-09-21 (the class-object flip): `Page.find_by_slog` against
/// `extend Findable` is a certain `NoMethodError` — MRI raises on this
/// exact file — and the class-object track now reports it. This was one
/// of the two rows where Sorbet proved a bug itaruby missed
/// (`scripts/inference-bench-README.md`); singleton lookup has already
/// proved the receiver's whole singleton chain closed and complete, and
/// the RBI wrapper leaves that `NotFound` unsoftened.
#[test]
fn extend_provided_class_method_typo_accuses() {
    assert_eq!(
        check_fixture("extend_typo_accuses.rb"),
        vec![
            "19:6:E0101 undefined method `find_by_slog` for class `Page`".to_string()
        ]
    );
}

/// CLOSED 2026-09-21, the Rails concern shape, same story: the typo on
/// `base.extend(ClassMethods)`'s injected name is a certain
/// `NoMethodError` and is now reported.
#[test]
fn included_hook_class_method_typo_accuses() {
    assert_eq!(
        check_fixture("included_hook_typo_accuses.rb"),
        vec![
            "22:8:E0101 undefined method `default_scope_nmae` for class `Record`".to_string()
        ]
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
