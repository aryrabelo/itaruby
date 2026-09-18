//! `ActiveSupport`'s `mattr_accessor` family (`cattr_*` is the same macro
//! under its older name) defines BOTH tracks: the class-object accessor
//! and, unless told otherwise, the instance accessor. `index.rs` modeled
//! neither until now — the name appeared zero times in the file — which is
//! why class methods like `ActiveRecord::Migrator.migrations_paths=` looked
//! absent from a class whose ancestry the index considered closed.
//!
//! STRICTLY ADDITIVE, and the openness half is the interesting part. Before
//! this arm existed the macro fell through to the `_` catch-all and opened
//! the class (`UnknownClassBodyCall`). Handling it must NOT close the class:
//! closing adds no knowledge, it only unmasks what the index already could
//! not see. Measured on discourse 2026-09-17 — `TopicQuery`'s only
//! class-body opener is `cattr_accessor :results_filter_callbacks`, and
//! closing it produced 28 new E0101 on methods that are real, installed by
//! `add_to_class(:topic_query, :list_group_topics_assigned)` from a plugin
//! file no index here can model. So the arm records the accessors and
//! leaves openness untouched: the public corpora move by exactly zero.
//!
//! That is why these tests assert on the INDEX rather than on diagnostics.
//! The diagnostic gain is gated on class openness, which is a separate
//! problem; what is banked here is the knowledge, and the proof that
//! banking it changed nothing observable.
//!
//! Every fixture in `testdata/mattr_accessor/` carries a faithful miniature
//! of the macro and RUNS, so MRI corroborates the semantics being modeled.

use itaruby_semantic::{project_index, Db, ProjectFiles, SourceFile};

/// What the index recorded for `Config` in one fixture: instance-track
/// names, class-object-track names, and whether the class is still open.
/// Returns plain data so no index-internal type has to be named here.
fn config_facts(name: &str) -> (Vec<String>, Vec<String>, bool) {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/mattr_accessor");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap();
    let db = Db::default();
    let file = SourceFile::new(&db, path.into(), text);
    ProjectFiles::new(&db, vec![file]);
    let index = project_index(&db);
    let id = *index.by_path.get("Config").expect("Config must be indexed");
    let class = index.class(id);
    let mut instance: Vec<String> = class.methods.keys().cloned().collect();
    let mut singleton: Vec<String> = class.singleton_methods.keys().cloned().collect();
    instance.sort();
    singleton.sort();
    (instance, singleton, class.open)
}

fn instance_methods(name: &str) -> Vec<String> {
    config_facts(name).0
}

fn singleton_methods(name: &str) -> Vec<String> {
    config_facts(name).1
}

/// `mattr_accessor` gives reader and writer on both tracks; `mattr_reader`
/// and `mattr_writer` give one side each; `cattr_accessor` is the alias.
#[test]
fn the_family_is_indexed_on_both_tracks() {
    let f = "accessors_resolve_silently.rb";
    assert_eq!(
        singleton_methods(f),
        vec!["endpoint", "endpoint=", "region", "retries", "retries=", "token="],
        "class-object track"
    );
    assert_eq!(
        instance_methods(f),
        vec!["endpoint", "endpoint=", "region", "retries", "retries=", "token="],
        "instance track"
    );
}

/// `instance_accessor: false` suppresses BOTH instance sides while the
/// class track survives. Inventing the instance reader here would hide a
/// real error: MRI raises on `Config.new.endpoint` in this fixture.
#[test]
fn instance_accessor_false_creates_no_instance_methods() {
    let f = "instance_accessor_false_has_no_instance_reader.rb";
    assert_eq!(singleton_methods(f), vec!["endpoint", "endpoint="]);
    assert!(
        instance_methods(f).is_empty(),
        "instance_accessor: false must create no instance accessor, got: {:?}",
        instance_methods(f)
    );
}

/// `instance_writer: false` removes only the instance writer — the reader
/// survives, matching MRI (which raises only on the instance assignment).
#[test]
fn instance_writer_false_keeps_the_instance_reader() {
    let f = "instance_writer_false_keeps_reader.rb";
    assert_eq!(singleton_methods(f), vec!["endpoint", "endpoint="]);
    assert_eq!(instance_methods(f), vec!["endpoint"]);
}

/// The openness contract, and the reason the public corpora did not move:
/// a class whose only class-body call is this macro stays exactly as open
/// as it was before the macro was understood. If this ever flips to
/// `false`, re-run `scripts/public-gate.sh` before believing it — the last
/// time the class closed, discourse gained 28 false positives.
#[test]
fn indexing_the_macro_does_not_close_the_class() {
    let (_instance, _singleton, open) = config_facts("accessors_resolve_silently.rb");
    assert!(open, "the macro must not close a class that used to be open");
}

/// CHARACTERIZATION of the price paid for that openness: a typo on an
/// instance accessor is a real `NoMethodError` (MRI raises in the fixture)
/// and itaruby stays silent, because a lookup on an open class is
/// Inconclusive. This pins CURRENT behavior, not desired behavior. The
/// index does hold the right names, which is what the fix banked; turning
/// that into a diagnostic needs the class to close, and closing it is the
/// thing that cost 28 false positives on discourse.
#[test]
fn the_typo_is_not_reported_because_the_class_stays_open() {
    let (instance, _singleton, open) = config_facts("typo_stays_silent_class_open.rb");
    assert!(open, "openness is the reason the typo goes unreported");
    assert_eq!(
        instance,
        vec!["endpoint", "endpoint="],
        "the accessor names ARE indexed - only the openness blocks the diagnostic"
    );
}
