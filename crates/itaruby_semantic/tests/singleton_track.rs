//! The singleton track: what a CLASS OBJECT really answers to, and which
//! populations the index must see before `MethodLookup::NotFound` on that
//! track could ever become a diagnostic (see
//! `crates/itaruby_semantic/tests/singleton_lookup.rs` for the measurement
//! that blocks reporting the residue today).
//!
//! Step 0 of that program measured the residue per corpus with an
//! instrumented build and bucketed it by the FAMILY of why the method
//! really exists (2026-09-17; explicit-receiver counts in parentheses):
//!
//! | family                        | rails    | mastodon | discourse   | corpus-c  |
//! |-------------------------------|----------|----------|-------------|-----------|
//! | `class << self` `attr_*`      | 0 (0)    | 0 (0)    | 0 (0)       | 0 (0)     |
//! | `extend`/`module_function`    | 68 (68)  | 14 (14)  | 196 (196)   | 0 (0)     |
//! | concern `ClassMethods`        | 0 (0)    | 0 (0)    | 4 (4)       | 0 (0)     |
//! | stdlib / `Module` surface     | 438 (357)| 19 (0)   | 844 (270)   | 12 (0)    |
//! | singleton provably OPEN       | 140 (117)| 0 (0)    | 315 (225)   | 1688 (2)  |
//! | gem-supplied receiver         | 242 (161)| 25 (22)  | 3443 (3413) | 1 (0)     |
//! | total                         | 888 (703)| 58 (36)  | 4803 (4109) | 1701 (2)  |
//!
//! Every family here is banked INDEX KNOWLEDGE, exactly like
//! `mattr_accessor.rs`: a name moves into a map some lookup consults, which
//! can only turn `NotFound`/`Inconclusive` into `Found` — never the
//! reverse, so the public and private error sets must not move at all
//! (invariant #1). What each step makes observable is the arity/sig
//! checking that rides on `Found`, and that is what these tests assert.
//!
//! Fixtures in `testdata/singleton_track/`, all MRI-executable: each
//! `_silently` fixture really exits 0 and each `_accuses` fixture really
//! raises on the blamed line.

use itaruby_semantic::{project_index, Db, ProjectFiles, SourceFile};

fn fixture(name: &str) -> (Db, SourceFile, String) {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/singleton_track");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap();
    let db = Db::default();
    let file = SourceFile::new(&db, path.into(), text.clone());
    ProjectFiles::new(&db, vec![file]);
    (db, file, text)
}

/// `line:col:code` for every diagnostic of one fixture, 1-based — the same
/// rendering the CLI prints.
fn diags(name: &str) -> Vec<String> {
    let (db, file, text) = fixture(name);
    let li = itaruby_semantic::LineIndex::new(&text);
    itaruby_semantic::check_file(&db, file)
        .iter()
        .map(|d| {
            let (l, c) = li.line_col(&text, d.start);
            format!("{}:{}:{}", l + 1, c + 1, d.code)
        })
        .collect()
}

/// (instance-track names, class-object-track names, still open?) for one
/// indexed path.
fn facts(name: &str, path: &str) -> (Vec<String>, Vec<String>, bool) {
    let (db, _file, _text) = fixture(name);
    let index = project_index(&db);
    let id = *index.by_path.get(path).unwrap_or_else(|| panic!("{path} must be indexed"));
    let class = index.class(id);
    let mut instance: Vec<String> = class.methods.keys().cloned().collect();
    let mut singleton: Vec<String> = class.singleton_methods.keys().cloned().collect();
    instance.sort();
    singleton.sort();
    (instance, singleton, class.open)
}

// ---------------------------------------------------------------------
// family (a): `attr_reader`/`attr_writer`/`attr_accessor` inside
// `class << self`
// ---------------------------------------------------------------------

/// The macro's methods belong to the class object. Before this step the
/// index filed them on the INSTANCE track, where no singleton lookup ever
/// looks — and MRI agrees they are not instance methods
/// (`sclass_attr_is_not_an_instance_method.rb` raises `NoMethodError`).
#[test]
fn sclass_attr_is_indexed_on_the_singleton_track() {
    let (instance, singleton, _open) = facts("sclass_attr_resolves_silently.rb", "Config");
    assert_eq!(
        singleton,
        vec!["endpoint", "endpoint=", "region", "token="],
        "class-object track"
    );
    assert!(instance.is_empty(), "instance track must stay empty, got {instance:?}");
}

/// The silence side: every call in the fixture is real, and MRI runs the
/// file to completion.
#[test]
fn sclass_attr_calls_resolve_silently() {
    assert!(
        diags("sclass_attr_resolves_silently.rb").is_empty(),
        "expected silence, got {:?}",
        diags("sclass_attr_resolves_silently.rb")
    );
}

/// What the banked knowledge makes observable: the singleton reader's
/// arity. MRI raises `ArgumentError: wrong number of arguments (given 1,
/// expected 0)` on exactly this line.
#[test]
fn sclass_attr_reader_arity_is_checked() {
    assert_eq!(diags("sclass_attr_arity_accuses.rb"), vec!["9:8:E0102"]);
}

// ---------------------------------------------------------------------
// family (b): `extend` copies a module's instance methods onto the
// extender's singleton — including `extend self` and `module_function`
// ---------------------------------------------------------------------

/// `extend self` is the one `extend` argument whose target is never in
/// doubt, and it used to open the module instead of resolving
/// (`OpenReason::DynamicMixinArg`). Now it is an `extend <own path>`
/// edge, so the module's own instance methods answer on the module
/// object — with their arity. MRI raises `ArgumentError: wrong number of
/// arguments (given 2, expected 1)` on the accusing fixture's last line.
#[test]
fn extend_self_exposes_instance_methods_on_the_module_object() {
    assert!(
        diags("extend_self_resolves_silently.rb").is_empty(),
        "expected silence, got {:?}",
        diags("extend_self_resolves_silently.rb")
    );
    assert_eq!(diags("extend_self_arity_accuses.rb"), vec!["11:6:E0102"]);
}

/// `module_function`, both shapes: the bare form (`ActionCable.server`'s
/// `module_function def server` and `Mastodon::Version.user_agent`'s bare
/// modifier were 49 and 2 measured residue sites) now resolves on the
/// module object. MRI raises on the accusing fixture's last line with the
/// same `given 2, expected 1`.
#[test]
fn module_function_exposes_methods_on_the_module_object() {
    assert!(
        diags("module_function_resolves_silently.rb").is_empty(),
        "expected silence, got {:?}",
        diags("module_function_resolves_silently.rb")
    );
    assert_eq!(diags("module_function_arity_accuses.rb"), vec!["11:6:E0102"]);
}

/// The edge is recorded as an `extend` of the module's own path, which is
/// what makes the existing `lookup_singleton` walk find the names with no
/// new mechanism. Pinned because the shape, not just the outcome, is the
/// contract the singleton walk depends on.
#[test]
fn the_self_extend_edge_names_the_modules_own_path() {
    let (db, _f, _t) = fixture("extend_self_resolves_silently.rb");
    let index = project_index(&db);
    let id = *index.by_path.get("Util").expect("Util must be indexed");
    assert_eq!(index.class(id).extends, vec!["Util".to_string()]);
}

// ---------------------------------------------------------------------
// family (c): `ActiveSupport::Concern`'s `ClassMethods`
// ---------------------------------------------------------------------

/// The Rails concern idiom: `extend ActiveSupport::Concern` plus a nested
/// `ClassMethods` module puts those methods on every includer's
/// singleton. Measured as 4 residue sites on discourse
/// (`AdminDashboardIndexData.fetch_cached_stats` via
/// `StatsCacheable::ClassMethods`), and it is the concern half of the gap
/// characterized in `singleton_lookup.rs`.
#[test]
fn concern_class_methods_reach_the_includers_singleton() {
    assert!(
        diags("concern_class_methods_resolves_silently.rb").is_empty(),
        "expected silence, got {:?}",
        diags("concern_class_methods_resolves_silently.rb")
    );
    assert_eq!(
        diags("concern_class_methods_arity_accuses.rb"),
        vec!["26:11:E0102"]
    );
}

/// The edge is attached to the CONCERN, which is why an ordinary include
/// ancestor walk finds it with no new lookup mechanism. Pinned because
/// the post-pass' shape is the contract: no nested `ClassMethods` in the
/// index means no edge, never a guess.
#[test]
fn the_concern_edge_is_only_added_when_class_methods_exists() {
    let (db, _f, _t) = fixture("concern_class_methods_resolves_silently.rb");
    let index = project_index(&db);
    let concern = *index.by_path.get("StatsCacheable").unwrap();
    assert!(
        index.class(concern).extends.contains(&"StatsCacheable::ClassMethods".to_string()),
        "concern must carry the ClassMethods edge, got {:?}",
        index.class(concern).extends
    );
    // `ActiveSupport::Concern` itself has no nested `ClassMethods` here,
    // and gains no edge.
    let plain = *index.by_path.get("ActiveSupport::Concern").unwrap();
    assert!(
        index.class(plain).extends.is_empty(),
        "no ClassMethods, no edge, got {:?}",
        index.class(plain).extends
    );
}

/// The block spelling, `class_methods do ... end`: the names land on a
/// synthetic `<concern>::ClassMethods` fragment with the same `extends`
/// edge, so one mechanism serves both spellings. Openness is PRESERVED
/// (the block still opens the concern, `mattr_accessor`'s measured
/// lesson), so what this banks is knowledge — the fixture stays silent
/// either way, and MRI runs it to completion.
#[test]
fn class_methods_block_is_harvested_as_the_class_methods_module() {
    let name = "class_methods_block_resolves_silently.rb";
    assert!(diags(name).is_empty(), "expected silence, got {:?}", diags(name));
    let (db, _f, _t) = fixture(name);
    let index = project_index(&db);
    let concern = *index.by_path.get("Countable").unwrap();
    assert!(
        index.class(concern).extends.contains(&"Countable::ClassMethods".to_string()),
        "edge missing, got {:?}",
        index.class(concern).extends
    );
    let cm = *index
        .by_path
        .get("Countable::ClassMethods")
        .expect("the block must synthesize the ClassMethods module");
    let mut names: Vec<String> = index.class(cm).methods.keys().cloned().collect();
    names.sort();
    assert_eq!(names, vec!["count_for"]);
    assert!(index.class(concern).open, "the concern stays open, as before");
}

// ---------------------------------------------------------------------
// step N+1, shape (1): the singleton reached by NAME rather than by
// lexical position — `X.singleton_class.prepend M` and `class << X`.
// ---------------------------------------------------------------------

/// The discourse shape, in miniature: a module prepended to a class's
/// SINGLETON class answers class-method calls. `FileScan` already saw
/// this call and filed it as an INSTANCE-track dynamic mixin (true of a
/// plain `prepend`, wrong through `singleton_class`), which is why 205
/// `DiscourseEvent.track_events` sites sat in the residue.
#[test]
fn singleton_class_prepend_lands_on_the_class_object() {
    let name = "singleton_class_prepend_resolves_silently.rb";
    assert!(diags(name).is_empty(), "expected silence, got {:?}", diags(name));
    let (db, _f, _t) = fixture(name);
    let index = project_index(&db);
    let bus = *index.by_path.get("Bus").unwrap();
    assert!(
        index.class(bus).extends.contains(&"BusTestHelper".to_string()),
        "expected the singleton edge, got {:?}",
        index.class(bus).extends
    );
}

/// And the other side: the name is now INDEXED, so a wrong-arity call to
/// it is reported. E0102 is the observable today — E0101 on this track is
/// still characterized as silent in `singleton_lookup.rs` — and MRI
/// raises `ArgumentError` on this exact line.
#[test]
fn singleton_class_prepend_arity_is_checked() {
    assert_eq!(diags("singleton_class_prepend_arity_accuses.rb"), vec!["18:5:E0102"]);
}

/// `class << X` with a constant expression defines on X's class object.
/// Both spellings inside the body count: a plain `def` and an `attr_*`.
#[test]
fn sclass_of_a_constant_defines_on_that_constants_singleton() {
    let name = "sclass_of_const_resolves_silently.rb";
    assert!(diags(name).is_empty(), "expected silence, got {:?}", diags(name));
    let (db, _f, _t) = fixture(name);
    let index = project_index(&db);
    let meter = *index.by_path.get("Meter").unwrap();
    let mut names: Vec<String> = index.class(meter).singleton_methods.keys().cloned().collect();
    names.sort();
    assert_eq!(names, vec!["calibrate", "unit", "unit="]);
}

/// Two-sided for that shape too.
#[test]
fn sclass_of_a_constant_arity_is_checked() {
    assert_eq!(diags("sclass_of_const_arity_accuses.rb"), vec!["11:7:E0102"]);
}

/// The measured trap, pinned: a by-name patch on a class the project does
/// NOT declare must invent nothing. The first version of
/// `apply_singleton_patches` interned the path, and discourse's
/// `TCPSocket.singleton_class.prepend` turned the stdlib class into a
/// closed, method-less project class — one new
/// E0101 "undefined method `close`" at
/// `spec/support/nginx_test_proxy.rb:145`, on code that runs. Silence
/// here is the whole point, and `by_path` must stay clean.
#[test]
fn a_patch_on_an_undeclared_class_invents_nothing() {
    let name = "singleton_patch_on_undeclared_class_invents_nothing.rb";
    assert!(diags(name).is_empty(), "expected silence, got {:?}", diags(name));
    let (db, _f, _t) = fixture(name);
    let index = project_index(&db);
    assert!(
        !index.by_path.contains_key("Time"),
        "the patch must not intern `Time`; paths: {:?}",
        index.by_path.keys().collect::<Vec<_>>()
    );
}
