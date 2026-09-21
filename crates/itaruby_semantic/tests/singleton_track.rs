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

/// The INSTANCE side of the same fact, through the checker's own
/// verdict: `class << self; attr_reader :x; end` defines `x` on the
/// CLASS OBJECT and on nothing else, so a call on an instance is a
/// certain `NoMethodError` (MRI raises on exactly this line of
/// `sclass_attr_is_not_an_instance_method.rb`) and the instance track
/// must keep accusing E0101 now that the name no longer sits there.
/// Before the singleton track existed, this name was filed on the
/// instance track and the call was silent — the silence of a checker
/// blind to the track was mistaken for correctness.
#[test]
fn sclass_attr_called_on_an_instance_accuses() {
    assert_eq!(
        diags("sclass_attr_is_not_an_instance_method.rb"),
        vec!["9:12:E0101"]
    );
}

/// `attr_*` was only the first spelling. `define_method`,
/// `alias_method` and the `alias` KEYWORD inside `class << self` define
/// class-object methods too, and the index filed all three on the
/// instance track — the same defect as family (a), one review later.
/// MRI runs `sclass_define_method_resolves_silently.rb` to completion.
#[test]
fn sclass_define_method_and_aliases_are_indexed_on_the_singleton_track() {
    let (instance, singleton, open) =
        facts("sclass_define_method_resolves_silently.rb", "Config");
    assert_eq!(
        singleton,
        vec!["other", "stats", "stats2", "stats3", "stats4"],
        "class-object track"
    );
    assert!(instance.is_empty(), "instance track must stay empty, got {instance:?}");
    assert!(!open, "a literal definer names what it defines: Config stays closed");
}

/// The silence side: every call in the fixture is real.
#[test]
fn sclass_define_method_calls_resolve_silently() {
    assert!(
        diags("sclass_define_method_resolves_silently.rb").is_empty(),
        "expected silence, got {:?}",
        diags("sclass_define_method_resolves_silently.rb")
    );
}

/// The observable side, through the instance track: a name
/// `define_method` installs inside `class << self` is NOT an instance
/// method, so this call is a certain `NoMethodError` (MRI raises on
/// line 15) and E0101 must report it. While the name sat on the
/// instance track the call was SILENT — the routing fix is what makes
/// this line accusable, and the mutant that reverts it makes this test
/// fail.
#[test]
fn sclass_define_method_called_on_an_instance_accuses() {
    assert_eq!(
        diags("sclass_define_method_is_not_an_instance_method.rb"),
        vec!["15:12:E0101"]
    );
}

// ---------------------------------------------------------------------
// Literal definers inside a `def` BODY. `body_def_reason` returns None
// for them — correctly, they are facts and not blankets — and until
// 2026-09-18 nothing consumed the fact, so `def self.install;
// define_singleton_method(:ready?) { true }; ... end` left the class
// CLOSED with none of the names it really installs. A guaranteed
// invariant #1 violation the moment the singleton `NotFound` arm
// reports, and discourse's `GlobalSetting` is exactly this shape.
// ---------------------------------------------------------------------

/// In a `def self.x` body `self` IS the class, so each definer's names
/// are filed on the track it really writes — and the class stays
/// CLOSED, because a literal definer names what it defines. MRI runs
/// the fixture to completion.
#[test]
fn def_body_literal_definers_are_filed_on_the_right_track() {
    let (instance, singleton, open) =
        facts("def_body_literal_definers_resolve_silently.rb", "Boot");
    assert_eq!(instance, vec!["mode", "mode=", "tick", "tock"], "instance track");
    assert_eq!(singleton, vec!["install", "ready?"], "class-object track");
    assert!(!open, "every definer in the body names what it defines");
    let d = diags("def_body_literal_definers_resolve_silently.rb");
    assert!(d.is_empty(), "expected silence, got {d:?}");
}

/// What the filing makes observable: `attr_accessor :mode` inside
/// `def self.install` defines a zero-argument reader, and MRI raises
/// `ArgumentError: wrong number of arguments (given 1, expected 0)` on
/// line 14. Before the filing the name was absent and the call silent.
#[test]
fn def_body_attr_accessor_arity_is_checked() {
    assert_eq!(diags("def_body_attr_accessor_arity_accuses.rb"), vec!["14:10:E0102"]);
}

/// An explicit constant receiver never registers onto the enclosing
/// class: `Other.define_method(:x)` inside `Host.install` is evidence
/// about Other, so Host learns nothing and Other — whose surface now
/// depends on someone calling `Host.install` — opens. Silence either
/// way; the failure mode this guards is a name landing on Host.
#[test]
fn def_body_foreign_literal_definer_stays_off_the_enclosing_class() {
    let (instance, singleton, open) =
        facts("def_body_foreign_literal_definer_resolves_silently.rb", "Host");
    assert!(instance.is_empty(), "Host must learn nothing, got {instance:?}");
    assert_eq!(singleton, vec!["install"]);
    assert!(!open, "Host's own surface is untouched by a foreign definer");
    let (other_instance, _other_singleton, other_open) =
        facts("def_body_foreign_literal_definer_resolves_silently.rb", "Other");
    assert!(other_instance.is_empty(), "the name is not filed onto Other either");
    assert!(other_open, "Other's surface is unknowable: it must fail closed");
    let d = diags("def_body_foreign_literal_definer_resolves_silently.rb");
    assert!(d.is_empty(), "expected silence, got {d:?}");
}

/// In an INSTANCE method body `self` is one object, not the class:
/// `define_singleton_method(:zoom)` there lands on that object alone.
/// The attribution is not provable, so the class fails CLOSED — open,
/// never enriched with a name only one instance answers to.
#[test]
fn def_body_definer_in_an_instance_method_fails_closed() {
    let (instance, singleton, open) =
        facts("def_body_instance_def_definer_resolves_silently.rb", "Widget");
    assert_eq!(instance, vec!["install"], "no dynamic name may be filed here");
    assert!(singleton.is_empty());
    assert!(open, "self is an instance: the class must open instead of collecting");
    let d = diags("def_body_instance_def_definer_resolves_silently.rb");
    assert!(d.is_empty(), "expected silence, got {d:?}");
}

// ---------------------------------------------------------------------

/// Bead ita-nst: a `def self.x` KEYWORD inside a plain-yield block inside
/// `def self.register!` (discourse's `EmotionDashboardReport` shape, 3
/// census residue sites that read as typos before this filing) defines on
/// the module's singleton at runtime — `self` at block-run time IS the
/// module. The walker files it on the singleton track.
#[test]
fn nested_def_self_in_block_files_on_the_singleton_track() {
    let (instance, singleton, open) =
        facts("nested_def_self_arity_accuses.rb", "Report");
    assert!(instance.is_empty(), "nothing on the instance track: {instance:?}");
    assert!(
        singleton.contains(&"fetch_data".to_string()),
        "the nested def must be filed: {singleton:?}"
    );
    assert!(singleton.contains(&"register!".to_string()));
    assert!(!open, "a literal nested def names what it defines");
}

/// The filing carries DATA, not just a name: the nested `def
/// self.fetch_data(x)` has one required parameter, so the wrong-arity
/// call on line 22 is an E0102 MRI really raises (ArgumentError, given 2
/// expected 1). Before the bead the call was silent residue.
#[test]
fn nested_def_self_arity_is_checked() {
    assert_eq!(diags("nested_def_self_arity_accuses.rb"), vec!["22:8:E0102"]);
}

/// A plain `def helper` inside an INSTANCE method's block defines on the
/// class of `self` — this very class — so the instance track files it.
#[test]
fn nested_plain_def_files_on_the_instance_track() {
    let (instance, singleton, open) =
        facts("nested_plain_def_files_instance_track.rb", "Maker");
    assert!(instance.contains(&"helper".to_string()), "instance track: {instance:?}");
    assert!(!open);
    let d = diags("nested_plain_def_files_instance_track.rb");
    assert!(d.is_empty(), "expected silence, got {d:?}");
}

/// `def self.bolt` inside an INSTANCE method's block defines on ONE
/// object's own singleton — an owner no index position can name. The
/// class fails CLOSED: open (`NestedDefOwner`), never enriched.
#[test]
fn nested_def_self_in_instance_body_fails_closed() {
    let (instance, singleton, open) =
        facts("nested_def_self_in_instance_body_opens.rb", "Maker");
    assert!(!instance.contains(&"bolt".to_string()), "no name may be filed: {instance:?}");
    assert!(!singleton.contains(&"bolt".to_string()));
    assert!(open, "the owner is unnameable: the class must open");
}

/// The same four definitions reached through `send`/`public_send`/
/// `__send__`, which is how a project reaches past a private definer.
/// One level of unwrapping and every rule above applies unchanged —
/// without it `body_def_reason` saw a plain unknown call, the class
/// stayed CLOSED, and none of the names it installs existed.
#[test]
fn def_body_send_wrapped_definers_are_filed_on_the_right_track() {
    let (instance, singleton, open) =
        facts("def_body_send_definers_resolve_silently.rb", "Boot");
    assert_eq!(instance, vec!["mode", "mode=", "tick", "tock"], "instance track");
    assert_eq!(singleton, vec!["install", "ready?"], "class-object track");
    assert!(!open, "the unwrapped call names what it defines");
    let d = diags("def_body_send_definers_resolve_silently.rb");
    assert!(d.is_empty(), "expected silence, got {d:?}");
}

/// The observable: `send(:attr_accessor, :mode)` defines a
/// zero-argument reader, and MRI raises `ArgumentError` on line 13.
#[test]
fn def_body_send_wrapped_attr_arity_is_checked() {
    assert_eq!(diags("def_body_send_attr_arity_accuses.rb"), vec!["13:10:E0102"]);
}

/// The other half of the unwrap, and the one that must FAIL CLOSED:
/// a dynamic name under the dispatch (`send(:define_method, key)`) is
/// exactly as unknowable as the bare `define_method(key)`, so the
/// class opens. Without the unwrap the call read as an ordinary
/// unknown send and the class stayed CLOSED with a surface it cannot
/// enumerate — the invariant #1 shape.
#[test]
fn def_body_send_wrapped_dynamic_name_opens_the_class() {
    let (_instance, _singleton, open) = facts("def_body_send_dynamic_name_opens.rb", "Boot");
    assert!(open, "a dynamic name under `send` must open the class");
    let d = diags("def_body_send_dynamic_name_opens.rb");
    assert!(d.is_empty(), "expected silence, got {d:?}");
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

// ---------------------------------------------------------------------
// step N+1, shapes (2)-(4): the class whose singleton surface cannot be
// proven closed must SAY so. Openness is not a diagnostic today — the
// `Ty::Class` `NotFound` arm is characterized silent in
// `singleton_lookup.rs` — so these assert the index FACT, plus the
// E0102 arity observable where one exists.
// ---------------------------------------------------------------------

/// Shape (2), the measured one: discourse's `GlobalSetting` installs its
/// class methods with `define_singleton_method(key)` inside a
/// `def self.*` body (`app/models/global_setting.rb:5`, `:69`, `:262`),
/// 275 residue sites. Def bodies are never walked, so the class looked
/// CLOSED with none of those names — silent today, an invariant #1
/// violation the moment the arm reports. MRI proves the method is real.
#[test]
fn a_dynamic_singleton_def_in_a_body_opens_the_class() {
    let name = "dynamic_singleton_def_in_body_opens.rb";
    assert!(diags(name).is_empty(), "expected silence, got {:?}", diags(name));
    let (_i, _s, open) = facts(name, "Settings");
    assert!(open, "a def body defining class methods dynamically must open the class");
}

/// The attribution rule, which is the whole reason this is not a text
/// scan: `Inner.class_eval { define_method(name) ... }` defines on
/// `Inner`. The enclosing class stays CLOSED and keeps being checked —
/// E0102 on line 20 proves it. Measured on rails: attributing that
/// nested implicit-receiver call to the enclosing class opened
/// `ActionDispatch::Routing::RouteSet` and swallowed a baseline E0101.
#[test]
fn a_foreign_eval_block_does_not_open_the_enclosing_class() {
    let name = "dynamic_def_in_body_only_opens_its_own_class.rb";
    assert_eq!(diags(name), vec!["20:7:E0102"]);
    let (_i, _s, open) = facts(name, "Outer");
    assert!(!open, "the enclosing class must stay closed");
}

/// Shape (3): `extend <local>`. Already handled by
/// `OpenReason::DynamicMixinArg` before step N+1 — pinned so a later
/// narrowing of that arm cannot silently close a class whose singleton
/// surface is chosen at runtime. MRI proves the module really answers.
#[test]
fn extend_of_a_non_constant_opens_the_class() {
    let name = "extend_non_constant_opens.rb";
    assert!(diags(name).is_empty(), "expected silence, got {:?}", diags(name));
    let (_i, _s, open) = facts(name, "Probe");
    assert!(open, "a runtime-chosen extend must open the class");
}

/// Shape (4): a singleton `method_missing`/`respond_to_missing?` answers
/// names no index can enumerate. Already handled (the `def` arm checks
/// the NAME before the track), pinned for the same reason.
#[test]
fn a_singleton_method_missing_opens_the_class() {
    let name = "singleton_method_missing_opens.rb";
    assert!(diags(name).is_empty(), "expected silence, got {:?}", diags(name));
    let (_i, _s, open) = facts(name, "Ghost");
    assert!(open, "a singleton method_missing must open the class");
}

/// The prefilter's coverage, pinned through behavior rather than
/// introspection: one class per shape `body_def_reason` reacts to, each
/// of which must end up OPEN. `BODY_DEF_NAMES` is a cheap substring gate
/// in front of the AST walk, and its first version omitted
/// `instance_eval` — measured as 10 MISSING discourse findings against
/// the unfiltered walk. A name dropped from that list is exactly this
/// test going red. Every line of the fixture really runs under MRI.
#[test]
fn every_dynamic_def_shape_opens_its_class() {
    let name = "dynamic_def_shapes_all_open.rb";
    assert!(diags(name).is_empty(), "expected silence, got {:?}", diags(name));
    for path in [
        "ByDefineMethod",
        "ByDefineSingletonMethod",
        "ByAliasMethod",
        "ByAttr",
        "ByClassEval",
        "ByInstanceEval",
        "ByModuleEval",
        "ByInstanceExec",
        "ByClassExec",
        "ByModuleExec",
    ] {
        let (_i, _s, open) = facts(name, path);
        assert!(open, "{path} must be open");
    }
}

/// Family (e): the stdlib class-object surface, end to end through the
/// checker rather than through `stdlib_singleton_method` directly (that
/// predicate is pinned two-sided in `stdlib_singletons.rs`). Every call
/// in the fixture really runs under MRI, which is what makes silence
/// here a correct answer rather than a convenient one — `FileUtils.*`
/// alone was 270 of discourse's explicit-receiver residue sites.
#[test]
fn the_stdlib_singleton_surface_is_silent() {
    let name = "stdlib_singleton_surface_silent.rb";
    assert!(diags(name).is_empty(), "expected silence, got {:?}", diags(name));
}

// ---------------------------------------------------------------------
// family (a), second spelling: `singleton_class.attr_reader/attr_writer/
// attr_accessor :a` — the RECEIVER form. The residue probe at de28b40
// measured it as the largest rails family (~90 of the 121
// explicit-receiver sites: ActiveSupport::Dependencies,
// ActionDispatch::ExceptionWrapper, ActiveModel::Translation, the
// ClassAttributeTest::Prepending fixture).
// ---------------------------------------------------------------------

/// The receiver form files on the class-object track, and on that track
/// ONLY — `singleton_class.attr_accessor :endpoint` defines nothing on
/// instances (MRI: `sclass_call_attr_typo_would_be_not_found.rb`'s
/// sibling raises on `Config.new.endpoint`).
#[test]
fn sclass_call_attr_is_indexed_on_the_singleton_track() {
    let (instance, singleton, open) = facts("sclass_call_attr_resolves_silently.rb", "Config");
    assert_eq!(
        singleton,
        vec!["connect", "endpoint", "endpoint=", "region", "token="]
    );
    assert!(instance.is_empty(), "instance track must stay empty, got {instance:?}");
    // OPENNESS IS PRESERVED at its pre-arm state: this spelling never
    // opened the class (the receiver early-return swallowed it), and
    // filing names must not start opening classes — opening only unmasks
    // what the index cannot see.
    assert!(!open, "the receiver spelling must not open the class");
}

/// The silence side: every call in the fixture is real and MRI runs the
/// file to completion (`ruby sclass_call_attr_resolves_silently.rb`
/// exits 0).
#[test]
fn sclass_call_attr_calls_resolve_silently() {
    let d = diags("sclass_call_attr_resolves_silently.rb");
    assert!(d.is_empty(), "expected silence, got {d:?}");
}

/// The filing is a FOUND, not an openness blanket: the wrong-arity call
/// rides the filed method into E0102. MRI raises
/// `ArgumentError: wrong number of arguments (given 1, expected 0)` on
/// exactly this line. This is also the mutant script's accusation for
/// the whole arm (`singleton-mutants.sh` MUT-A removes the filing and
/// this test must fail).
#[test]
fn sclass_call_attr_reader_arity_is_checked() {
    assert_eq!(diags("sclass_call_attr_arity_accuses.rb"), vec!["7:8:E0102"]);
}

/// The ground-truth side of the typo: MRI raises `NoMethodError` on
/// `Config.endpointt` (line 5). The checker stays silent — the
/// singleton `NotFound` arm is characterized in
/// `singleton_lookup.rs` — but the fixture pins WHY a filed name
/// matters: without the filing this call site is indistinguishable
/// from the typo by the index.
#[test]
fn sclass_call_attr_typo_stays_characterized_silent() {
    let d = diags("sclass_call_attr_typo_would_be_not_found.rb");
    assert!(d.is_empty(), "expected characterized silence, got {d:?}");
}

/// The concern edge is the gate: a module that calls `class_methods do`
/// WITHOUT extending `ActiveSupport::Concern` must not have a
/// `ClassMethods` module invented for it — no fragment in `by_path`, no
/// extends edge, no filed methods. An invented CLOSED surface is
/// exactly what the flip could never be allowed to accuse against; the
/// module itself still opens (the `_` catch-all, unchanged silence).
/// MRI ground truth for the idiom lives in the sibling concern fixture:
/// there the gem really defines `ClassMethods` from the block.
#[test]
fn a_non_concern_class_methods_block_invents_nothing() {
    let name = "non_concern_class_methods_invents_nothing.rb";
    let (db, _file, _text) = fixture(name);
    let index = project_index(&db);
    assert!(
        !index.by_path.contains_key("Plain::ClassMethods"),
        "the harvest must not invent ClassMethods without the concern edge"
    );
    let id = *index.by_path.get("Plain").expect("the module itself is indexed");
    let class = index.class(id);
    assert!(
        !class.extends.iter().any(|e| e.ends_with("ClassMethods")),
        "no extends edge may be invented either"
    );
    assert_eq!(
        class.singleton_methods.keys().cloned().collect::<Vec<_>>(),
        vec!["class_methods"],
        "the fixture's own def self.class_methods is filed; nothing else may be"
    );
    assert!(class.open, "the catch-all's openness must be preserved");
    let d = diags(name);
    assert!(d.is_empty(), "expected silence, got {d:?}");
}

// ---------------------------------------------------------------------
// `class_attribute :a` (ActiveSupport). Read out of the gem's own
// source: singleton reader/writer always, `a?` predicate unless
// `instance_predicate: false`, instance reader/writer behind the
// documented option chain. OPENNESS PRESERVED like the mattr family:
// receiverless today falls into the `_` catch-all, so the class was
// and stays OPEN — the filing is banked knowledge, and its resolution
// gain is gated on class openness, which is a separate problem.
// ---------------------------------------------------------------------

/// Reader, writer, and predicate land on the class-object track; the
/// instance track carries reader, writer, and predicate; and the class
/// is exactly as open as the catch-all left it.
#[test]
fn class_attribute_is_filed_on_both_tracks() {
    let (instance, singleton, open) = facts("class_attribute_resolves_silently.rb", "Base");
    assert_eq!(singleton, vec!["report", "setting", "setting=", "setting?"]);
    assert_eq!(instance, vec!["setting", "setting=", "setting?"]);
    assert!(open, "the catch-all's openness must be preserved");
}

/// Every call in the fixture is real under MRI (the inline
/// implementation mirrors the gem's semantics; `ruby
/// class_attribute_resolves_silently.rb` exits 0).
#[test]
fn class_attribute_calls_resolve_silently() {
    let d = diags("class_attribute_resolves_silently.rb");
    assert!(d.is_empty(), "expected silence, got {d:?}");
}

/// `instance_predicate: false` (literal) removes the predicate from
/// BOTH tracks — MRI raises `NoMethodError` on `Base.setting?` (line 28).
/// This is also the mutant script's accusation for the predicate
/// decision (`singleton-mutants.sh` MUT-B removes the predicate filing
/// and this test must fail).
#[test]
fn class_attribute_instance_predicate_false_removes_the_predicate() {
    let (instance, singleton, _open) =
        facts("class_attribute_instance_predicate_false_raises.rb", "Base");
    assert_eq!(singleton, vec!["setting", "setting="]);
    assert_eq!(instance, vec!["setting", "setting="]);
}

/// `instance_reader: false` (literal) removes the instance reader and,
/// with it, the instance predicate — the singleton track is untouched
/// (the gem defines the class sides regardless of every instance_*
/// option). MRI raises `NoMethodError` on `Base.new.setting` (line 28).
#[test]
fn class_attribute_instance_reader_false_removes_the_instance_reader() {
    let (instance, singleton, _open) =
        facts("class_attribute_instance_reader_false_raises.rb", "Base");
    assert_eq!(singleton, vec!["setting", "setting=", "setting?"]);
    assert_eq!(instance, vec!["setting="]);
}

/// A DYNAMIC option value is read conservatively: every side that may
/// exist is filed. MRI runs the fixture both ways (`ruby ...` and
/// `ruby ... off` both exit 0) because at runtime the flag decides.
#[test]
fn class_attribute_dynamic_option_defines_everything() {
    let (instance, singleton, _open) =
        facts("class_attribute_dynamic_option_defines_everything.rb", "Base");
    assert_eq!(singleton, vec!["setting", "setting=", "setting?"]);
    assert_eq!(instance, vec!["setting", "setting=", "setting?"]);
    let d = diags("class_attribute_dynamic_option_defines_everything.rb");
    assert!(d.is_empty(), "expected silence, got {d:?}");
}

// ---------------------------------------------------------------------
// `thread_mattr_accessor` and friends — the same macro family as
// `mattr_accessor` with a thread-local backing store, so they join the
// mattr arm's option parsing.
// ---------------------------------------------------------------------

/// The thread variants file both tracks exactly like `mattr_accessor`:
/// the singleton reader/writer always, the instance reader/writer
/// behind `instance_reader`/`instance_writer` AND `instance_accessor`.
/// MRI runs the fixture to completion.
#[test]
fn thread_mattr_accessor_is_indexed_on_both_tracks() {
    let (instance, singleton, open) =
        facts("thread_mattr_accessor_resolves_silently.rb", "Job");
    assert_eq!(singleton, vec!["drain", "queue", "queue="]);
    assert_eq!(instance, vec!["queue", "queue="]);
    assert!(open, "the catch-all's openness must be preserved");
    let d = diags("thread_mattr_accessor_resolves_silently.rb");
    assert!(d.is_empty(), "expected silence, got {d:?}");
}

// ---------------------------------------------------------------------
// MRI ground truth, EXECUTED. Until 2026-09-18 every "MRI raises on
// this line" / "MRI runs the file to completion" claim in this file
// lived in a doc comment — narrated, never run, which is exactly the
// shape AGENTS.md refuses ("the probes proving each side live next to
// the instrument, never only narrated"). The table below runs each
// fixture and pins the SAME line numbers the diagnostic assertions
// above pin, which is what makes this a check on the diagnostic rather
// than on Ruby, and its closure check forces a new fixture to declare
// what MRI does with it instead of arriving unmeasured.
// ---------------------------------------------------------------------

enum Mri {
    /// Runs to completion, exit 0.
    Clean,
    /// Raises `kind` on this 1-based line.
    Raises(&'static str, u32),
}

fn fixture_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/singleton_track"
    ))
}

fn ruby_available() -> bool {
    std::process::Command::new("ruby")
        .arg("-e")
        .arg("exit 0")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// (exit code, stdout+stderr) of `ruby <path> [args...]`.
fn run_ruby(path: &std::path::Path, args: &[&str]) -> (i32, String) {
    let out = std::process::Command::new("ruby")
        .arg(path)
        .args(args)
        .output()
        .expect("ruby must be spawnable");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

fn assert_mri(name: &str, expected: &Mri) {
    let (code, text) = run_ruby(&fixture_dir().join(name), &[]);
    match expected {
        Mri::Clean => assert_eq!(code, 0, "{name} must run clean: {text}"),
        Mri::Raises(kind, line) => {
            assert_eq!(code, 1, "{name} must fail: {text}");
            assert!(text.contains(kind), "{name} must raise {kind}: {text}");
            let blamed = format!("{name}:{line}:in ");
            assert!(text.contains(&blamed), "{name} must raise on line {line}: {text}");
        }
    }
}

#[test]
fn mri_ground_truth_is_executed() {
    if !ruby_available() {
        eprintln!("skipping: no `ruby` on PATH — the MRI leg is machine-dependent");
        return;
    }
    let table = [
        ("class_attribute_dynamic_option_defines_everything.rb", Mri::Clean),
        ("class_attribute_instance_predicate_false_raises.rb", Mri::Raises("NoMethodError", 28)),
        ("class_attribute_instance_reader_false_raises.rb", Mri::Raises("NoMethodError", 28)),
        ("class_attribute_resolves_silently.rb", Mri::Clean),
        ("class_methods_block_resolves_silently.rb", Mri::Clean),
        ("concern_class_methods_arity_accuses.rb", Mri::Raises("ArgumentError", 16)),
        ("concern_class_methods_resolves_silently.rb", Mri::Clean),
        ("def_body_attr_accessor_arity_accuses.rb", Mri::Raises("ArgumentError", 14)),
        ("def_body_foreign_literal_definer_resolves_silently.rb", Mri::Clean),
        ("def_body_instance_def_definer_resolves_silently.rb", Mri::Clean),
        ("def_body_literal_definers_resolve_silently.rb", Mri::Clean),
        ("def_body_send_attr_arity_accuses.rb", Mri::Raises("ArgumentError", 13)),
        ("def_body_send_definers_resolve_silently.rb", Mri::Clean),
        ("def_body_send_dynamic_name_opens.rb", Mri::Clean),
        ("dynamic_def_in_body_only_opens_its_own_class.rb", Mri::Raises("ArgumentError", 11)),
        ("dynamic_def_shapes_all_open.rb", Mri::Clean),
        ("dynamic_singleton_def_in_body_opens.rb", Mri::Clean),
        ("extend_non_constant_opens.rb", Mri::Clean),
        ("extend_self_arity_accuses.rb", Mri::Raises("ArgumentError", 6)),
        ("extend_self_resolves_silently.rb", Mri::Clean),
        ("module_function_arity_accuses.rb", Mri::Raises("ArgumentError", 6)),
        ("module_function_resolves_silently.rb", Mri::Clean),
        ("non_concern_class_methods_invents_nothing.rb", Mri::Clean),
        ("sclass_attr_arity_accuses.rb", Mri::Raises("ArgumentError", 9)),
        ("sclass_attr_is_not_an_instance_method.rb", Mri::Raises("NoMethodError", 9)),
        ("sclass_attr_resolves_silently.rb", Mri::Clean),
        ("sclass_call_attr_arity_accuses.rb", Mri::Raises("ArgumentError", 7)),
        ("sclass_call_attr_resolves_silently.rb", Mri::Clean),
        ("sclass_call_attr_typo_would_be_not_found.rb", Mri::Raises("NoMethodError", 5)),
        ("sclass_define_method_is_not_an_instance_method.rb", Mri::Raises("NoMethodError", 15)),
        ("sclass_define_method_resolves_silently.rb", Mri::Clean),
        ("sclass_of_const_arity_accuses.rb", Mri::Raises("ArgumentError", 6)),
        ("sclass_of_const_resolves_silently.rb", Mri::Clean),
        ("singleton_class_prepend_arity_accuses.rb", Mri::Raises("ArgumentError", 11)),
        ("singleton_class_prepend_resolves_silently.rb", Mri::Clean),
        ("singleton_method_missing_opens.rb", Mri::Clean),
        ("singleton_patch_on_undeclared_class_invents_nothing.rb", Mri::Clean),
        ("stdlib_singleton_surface_silent.rb", Mri::Clean),
        ("nested_def_self_arity_accuses.rb", Mri::Raises("ArgumentError", 22)),
        ("nested_def_self_in_instance_body_opens.rb", Mri::Clean),
        ("nested_plain_def_files_instance_track.rb", Mri::Clean),
        ("thread_mattr_accessor_resolves_silently.rb", Mri::Clean),
    ];
    for (name, expected) in &table {
        assert_mri(name, expected);
    }
    // The dynamic-option fixture's whole point is that the runtime flag
    // decides, so BOTH argument vectors must run clean.
    let (code, text) = run_ruby(
        &fixture_dir().join("class_attribute_dynamic_option_defines_everything.rb"),
        &["off"],
    );
    assert_eq!(code, 0, "the dynamic-option fixture must run clean with `off` too: {text}");
    // Every fixture on disk is in the table.
    let mut on_disk: Vec<String> = std::fs::read_dir(fixture_dir())
        .expect("fixture dir")
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "rb"))
        .map(|p| p.file_name().expect("file name").to_string_lossy().into_owned())
        .collect();
    on_disk.sort();
    let mut declared: Vec<String> = table.iter().map(|(n, _)| (*n).to_string()).collect();
    declared.sort();
    assert_eq!(on_disk, declared, "every fixture must declare its MRI outcome");
}
