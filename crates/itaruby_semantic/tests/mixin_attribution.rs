//! The attributed-mixin family (three independent mechanisms, all
//! suppression-only — invariant #1, `Ty::Unknown` never accuses):
//!
//! 1. A mixin edge whose RECEIVER this index can name carries the
//!    receiver's `method_missing` openness: `X.include(M)` with a literal
//!    constant receiver, or the value a resolvable expression holds.
//!    Keyed on the RECEIVER, never on "some module has `method_missing`" —
//!    the receiver-blind form silenced 100% of two corpora (AGENTS.md,
//!    bead ita-a8z), which `unattributed_dynamic_receiver_still_accuses`
//!    pins.
//! 2. The receiver may be a local assigned a project method's return, and
//!    that method's body a ternary of constant paths — the Rails
//!    `builder_class = get_builder_class` /
//!    `defined?(::AppBuilder) ? ::AppBuilder : Rails::AppBuilder` shape.
//!    Every candidate constant is attributed, never "some class of the
//!    program"; a class the ternary never names keeps its accusation
//!    (`ternary_without_method_missing_still_accuses`).
//! 3. `%w(a b).each { |m| class_eval <<-RUBY def #{m} ... RUBY }` files
//!    exactly the names the literal list spells — the only way a `def`
//!    written inside an eval string becomes visible. Bounded to those
//!    names: an unlisted name still accuses
//!    (`interpolated_eval_unlisted_name_still_accuses`).
//!
//! Measured against the pinned rails clone (3df2cbea, release binary):
//! this family is 160 of rails' 166 baseline E0101 — mechanism 2 closes
//! the 160-site `Rails::AppBuilder`/`Rails::PluginBuilder` cluster.
//! Mechanism 3's own marginal is ZERO in that configuration (rails lands
//! on the same 6 with it and without it, re-measured 2026-09-19; the
//! "95 sites" it used to be credited with was it measured WITHOUT
//! mechanism 2, which is not a configuration that ships) — it is kept for
//! the shape its own fixture proves against MRI, where the receiver is a
//! method PARAMETER and no receiver-keyed mechanism can attribute
//! anything. mastodon and discourse stay byte-identical.
//!
//! Every mechanism here is INSTANCE-track: `X.extend(M)` lands `M` on X's
//! singleton and is deliberately not attributed
//! (`singleton_track_stays_closed`). The mirror direction has no verdict
//! to break — this checker emits no singleton `NotFound` for a project
//! class at all (probed 2026-09-19).
//!
//! Fixtures live in `testdata/mixin_attribution/`, each with globally
//! unique `MixAttr`-prefixed class/module names (`ita check testdata/`
//! merges the whole tree into one project — gate c) and each really
//! MRI-executable: every `_resolves_silently` fixture exits 0 and every
//! `_accuses` fixture raises `NoMethodError`/`NameError` on the blamed
//! line.

use itaruby_semantic::{project_index, Db, OpenReason, ProjectFiles, SourceFile};

fn fixture(name: &str) -> (Db, SourceFile) {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/mixin_attribution");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap();
    let db = Db::default();
    let file = SourceFile::new(&db, path.into(), text);
    ProjectFiles::new(&db, vec![file]);
    (db, file)
}

fn diags(name: &str) -> Vec<String> {
    let (db, file) = fixture(name);
    itaruby_semantic::check_file(&db, file)
        .iter()
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect()
}

/// Two files in ONE project — what mechanism 2's key-by-NAME is built on:
/// the method whose return names the receiver is defined in another file
/// than the mixin call site that consumes it.
fn two_file_fixture(definition: &str, call_site: &str) -> (Db, SourceFile) {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/mixin_attribution");
    let db = Db::default();
    let load = |name: &str| {
        let path = format!("{dir}/{name}");
        let text = std::fs::read_to_string(&path).unwrap();
        SourceFile::new(&db, path.into(), text)
    };
    let def = load(definition);
    let site = load(call_site);
    ProjectFiles::new(&db, vec![def, site]);
    (db, site)
}

/// `(open, open_reason)` for one indexed path.
fn class_open(name: &str, path: &str) -> (bool, Option<OpenReason>) {
    let (db, _file) = fixture(name);
    let index = project_index(&db);
    let id = *index.by_path.get(path).unwrap_or_else(|| panic!("{path} must be indexed"));
    let class = index.class(id);
    (class.open, class.open_reason)
}

// ---------------------------------------------------------------------
// Mechanism 1: a literal-receiver mixin edge, method_missing follows it.
// ---------------------------------------------------------------------

/// SILENT: `MixAttrBuilder.include(MixAttrForwarding)` names its receiver,
/// the module answers every name, so the receiver is `open` and
/// `mix_attr_run` is not a `NotFound`.
#[test]
fn attributed_literal_receiver_is_silenced() {
    let diags = diags("attributed_literal_receiver_resolves_silently.rb");
    assert!(diags.is_empty(), "expected no diagnostics, got: {diags:?}");
}

/// The reason, pinned: the receiver is open BECAUSE of the mixed-in
/// `method_missing`, so a mutant that opens it for some unrelated reason
/// is not silently accepted here.
#[test]
fn attributed_literal_receiver_opens_with_method_missing() {
    let (open, reason) = class_open("attributed_literal_receiver_resolves_silently.rb", "MixAttrBuilder");
    assert!(open, "MixAttrBuilder must be open");
    assert_eq!(reason, Some(OpenReason::MethodMissing), "open for the mixed-in method_missing");
}

/// FIRES (control): the SAME `method_missing` module mixed in through an
/// unnamable receiver attributes nothing — the class stays closed and
/// the call is a real E0101. This is the receiver-blind failure shape
/// (AGENTS.md, bead ita-a8z) held down.
#[test]
fn unattributed_dynamic_receiver_still_accuses() {
    let diags = diags("unattributed_dynamic_receiver_still_accuses.rb");
    let hits: Vec<_> = diags.iter().filter(|d| d.contains("mix_attr_unattributed_run")).collect();
    assert_eq!(hits.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(hits[0].contains("E0101"), "expected E0101, got: {hits:?}");
}

// ---------------------------------------------------------------------
// Mechanism 2: the ternary-of-constants receiver, every candidate.
// ---------------------------------------------------------------------

/// SILENT: `builder_class = mix_attr_get_builder_class` holds one of two
/// constant paths, and the module answers every name — so BOTH candidate
/// classes are opened. Neither call site is a `NotFound`, including the
/// else-arm class MRI never instantiates here.
#[test]
fn attributed_ternary_receiver_is_silenced() {
    let diags = diags("attributed_receiver_from_ternary_resolves_silently.rb");
    assert!(diags.is_empty(), "expected no diagnostics, got: {diags:?}");
}

/// Both candidate classes open — the then-arm AND the else-arm. A mutant
/// that reads only one arm of the ternary leaves the other closed and
/// this flips.
#[test]
fn attributed_ternary_opens_every_candidate() {
    for path in ["MixAttrTernaryPluginBuilder", "MixAttrTernaryAppBuilder"] {
        let (open, reason) = class_open("attributed_receiver_from_ternary_resolves_silently.rb", path);
        assert!(open, "{path} must be open");
        assert_eq!(reason, Some(OpenReason::MethodMissing), "{path} open for method_missing");
    }
}

/// FIRES (control): the receiver is attributed exactly the same way, but
/// the module answers no `method_missing` — so opening the receiver is
/// NOT licensed, and a name it neither defines nor forwards accuses on
/// both candidate classes.
#[test]
fn ternary_without_method_missing_still_accuses() {
    let diags = diags("ternary_without_method_missing_still_accuses.rb");
    let hits: Vec<_> = diags.iter().filter(|d| d.contains("mix_attr_gated_absent")).collect();
    assert_eq!(hits.len(), 2, "expected both candidate classes to accuse, got: {diags:?}");
    assert!(hits.iter().all(|d| d.contains("E0101")), "expected E0101, got: {hits:?}");
}

// ---------------------------------------------------------------------
// Mechanism 3: the interpolated-`def` harvest.
// ---------------------------------------------------------------------

/// SILENT: `mix_attr_harvest_template` is written only inside a
/// `class_eval` heredoc, invisible to any per-`def`-node scan; harvested
/// from the literal list, it softens through the dynamic mixin.
#[test]
fn interpolated_eval_defs_are_harvested() {
    let diags = diags("interpolated_eval_defs_resolve_silently.rb");
    assert!(diags.is_empty(), "expected no diagnostics, got: {diags:?}");
}

/// The names are FILED, not blanketed: the module's own `methods` map
/// carries exactly the harvested names. A mutant that files nothing
/// leaves this map without them (and the softening above with nothing to
/// find).
#[test]
fn interpolated_eval_names_are_filed_on_the_module() {
    let (db, _file) = fixture("interpolated_eval_defs_resolve_silently.rb");
    let index = project_index(&db);
    let id = *index.by_path.get("MixAttrHarvestActionMethods").expect("module indexed");
    let names = &index.class(id).methods;
    for name in ["mix_attr_harvest_template", "mix_attr_harvest_copy_file"] {
        assert!(names.contains_key(name), "expected `{name}` harvested onto the module, got: {:?}", names.keys().collect::<Vec<_>>());
    }
}

/// FIRES (control): a name the literal list never spells is not
/// harvested, so a class mixing the module in still accuses on it — the
/// harvest is bounded to the listed names, never a blanket over every
/// call on any includer.
#[test]
fn interpolated_eval_unlisted_name_still_accuses() {
    let diags = diags("interpolated_eval_unlisted_name_accuses.rb");
    let hits: Vec<_> = diags.iter().filter(|d| d.contains("mix_attr_control_absent")).collect();
    assert_eq!(hits.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(hits[0].contains("E0101"), "expected E0101, got: {hits:?}");
}

// ---------------------------------------------------------------------
// The track: `extend` is deliberately NOT attributed, and the eval
// harvest files names only where the body really runs.
// ---------------------------------------------------------------------

/// FIRES (control): `extend` lands the module on the receiver's
/// SINGLETON, so the extender's INSTANCE lookups must keep accusing even
/// though the module answers every name. The family attributes
/// `include`/`prepend` only; a mutant that re-adds the `extend` arm opens
/// the class and this flips.
#[test]
fn singleton_track_stays_closed() {
    let diags = diags("singleton_track_stays_closed.rb");
    let hits: Vec<_> = diags.iter().filter(|d| d.contains("mix_attr_track_extended_absent")).collect();
    assert_eq!(hits.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(hits[0].contains("E0101"), "expected E0101, got: {hits:?}");
}

/// The eval call's receiver decides where the harvested names go:
/// `Other.class_eval` inside `class Bar` defines methods on OTHER, so Bar
/// must not gain them. Pinned at the INDEX level on purpose — the
/// diagnostic side cannot tell the two behaviours apart, because the
/// enclosing class opens for the `class_eval` call itself
/// (`OpenReason::EvalOrSend`) and the call is silent either way
/// (measured 2026-09-19).
#[test]
fn foreign_class_eval_defs_are_not_filed_here() {
    let (db, _file) = fixture("foreign_class_eval_defs_are_not_filed_here.rb");
    let index = project_index(&db);
    let bar = *index.by_path.get("MixAttrForeignEvalBar").expect("Bar indexed");
    assert!(
        !index.class(bar).methods.contains_key("mix_attr_foreign_eval_absent"),
        "a name defined by `Other.class_eval` must not be filed on the enclosing class, got: {:?}",
        index.class(bar).methods.keys().collect::<Vec<_>>()
    );
}

/// SILENT across files: the method whose return names the receiver lives
/// in `..._definition.rb`, the call site that consumes it in
/// `..._call_site.rb`. Phase 2 resolves `MixinReceiver::Call` against the
/// MERGED `const_returning_methods` map, so the builder is opened even
/// though nothing in the call site's own file says what the receiver is —
/// which is exactly the rails shape the key-by-NAME exists for. No
/// single-decision mutant covers this one (scoping the lookup to the call
/// site's file is not a text-level change), so the fixture is the proof.
#[test]
fn attributed_receiver_resolves_across_files() {
    let (db, site) = two_file_fixture(
        "attributed_receiver_across_files_definition.rb",
        "attributed_receiver_across_files_call_site.rb",
    );
    let diags: Vec<String> = itaruby_semantic::check_file(&db, site)
        .iter()
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    assert!(diags.is_empty(), "expected no diagnostics, got: {diags:?}");
}
