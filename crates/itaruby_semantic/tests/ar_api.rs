//! Bead ita-k9j, entrega 2: `Checker::rbi_escalate`'s third step —
//! `declarations/activerecord_api.txt` (`AR_API_INSTANCE`/
//! `AR_API_SINGLETON`), consulted only when a project class's ancestry
//! carries a declared-external `ActiveRecord::Base` (`ProjectIndex::
//! declared_by`) and the DSL/gem RBI walks already missed. Additive-only,
//! same contract as `rbi_methods.rs`'s population 2: a hit resolves to
//! `Ty::Unknown` and nothing else (no `MethodDef`, so arity is never
//! checked), and a miss changes nothing observable — the call stays
//! `Inconclusive`, exactly as it was before this bead.
//!
//! Fixtures under `testdata/ar_api/`, each with a globally unique
//! `ArApi`-prefixed class name (`ita check testdata/` scans the whole tree
//! as one project — a repeated name would merge ancestry across fixtures
//! and leak diagnostics between files, bead ita-u1t).
//!
//! MUTANTS THIS FILE MUST CATCH (see `check.rs::Checker::rbi_escalate`'s
//! step 3 and `AR_API_INSTANCE`/`AR_API_SINGLETON`):
//!
//!   1. step 3's `if hit { ... }` guard removed, so a declared-external
//!      `ActiveRecord::Base` ancestor always resolves regardless of the
//!      callee name -> `name_outside_inventory_never_manufactures_diagnostic`
//!      fails (`ar_api_method` becomes 1 instead of 0, `not_ar_api` becomes
//!      0 instead of 1) — this is the regression lock for "never
//!      manufactures a hit out of a name that isn't real API".
//!   2. step 3 deleted entirely (reverts `rbi_escalate` to its pre-entrega-2
//!      two-step shape) -> `class_level_api_escalates_silently` and
//!      `instance_level_api_escalates_silently` both fail (`ar_api_method`
//!      stays 0, the call sites fall back to `inconclusive`/`not_ar_api`).
//!   3. `AR_API_INSTANCE`/`AR_API_SINGLETON` swapped in the `hit` check ->
//!      the same two tests fail (`.where` checked against the instance set,
//!      `.save` checked against the singleton set — neither name is in the
//!      wrong set, so both miss and `ar_api_method` stays 0).
//!   4. escalation tried BEFORE the project's own `lookup_method_rbi` (the
//!      `Ty::Instance(c)` arm calls `rbi_escalate` first, falling back to
//!      `lookup_method_rbi` only on a miss) -> `project_definition_wins_over_curated_inventory`
//!      fails on its `ar_api_method` assertion: `.save` now goes through
//!      the curated inventory instead of falling through to the project's
//!      own `def save`. Verified empirically that `definition_at` alone
//!      does NOT catch this mutant — its silent walk (`self.silent`)
//!      makes `rbi_escalate` return `None` unconditionally regardless of
//!      where in the match it is called, so `definition_at` still finds
//!      `def save` either way; only the non-silent `call_stats` walk
//!      exposes the reordering, which is exactly why this test asserts
//!      on both.
//!
//! All four were applied by hand, run, confirmed to fail exactly as
//! predicted, and reverted — see the entrega 2 report for the transcript.

fn dir() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/ar_api")
}

fn fixture_text(name: &str) -> (String, String) {
    let path = format!("{}/{name}", dir());
    let text = std::fs::read_to_string(&path).unwrap();
    (path, text)
}

/// Single-file project, same shape as `const_ancestors.rs`/`block_self.rs`:
/// each fixture is its own isolated project, independent of every other
/// file under `testdata/ar_api/`.
fn check_fixture(name: &str) -> Vec<String> {
    let (path, text) = fixture_text(name);
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

fn stats_fixture(name: &str) -> itaruby_semantic::CallStats {
    let (path, text) = fixture_text(name);
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, path.into(), text);
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    itaruby_semantic::call_stats(&db, file)
}

/// Byte offset of `ident` where it's reached via the unique `needle` prefix
/// — same helper as `definition.rs`'s `offset_in`, avoiding a match on an
/// unrelated earlier occurrence of the bare identifier.
fn offset_in(text: &str, needle: &str, ident: &str) -> usize {
    let start = text.find(needle).unwrap_or_else(|| panic!("`{needle}` not found in fixture"));
    start + needle.rfind(ident).expect("ident must be a suffix of needle")
}

/// `Model.where(id: 1)`: `Ty::Class` receiver, `ActiveRecord::Base`
/// declared-external, `where` is in `AR_API_SINGLETON` (`activerecord_api.txt`
/// line 1181) — `rbi_escalate`'s third step must resolve it, both to
/// silence (invariant #1: `Inconclusive` never diagnosed anyway, so
/// silence alone would pass even with the step entirely deleted — mutant
/// 2) AND to the `ar_api_method` bucket specifically (which mutant 2
/// disproves).
#[test]
fn class_level_api_escalates_silently() {
    let diags = check_fixture("class_level_silent.rb");
    assert!(diags.is_empty(), "`.where` is real ActiveRecord::Base class API: expected silence, got {diags:?}");

    let s = stats_fixture("class_level_silent.rb");
    assert_eq!(s.ar_api_method, 1, "`.where` must be tallied as the curated-inventory hit: {s:?}");
    assert_eq!(s.inconclusive, 0, "a resolved hit must not also count as inconclusive: {s:?}");
    assert_eq!(s.total(), 1, "exactly one call site in the fixture: {s:?}");
}

/// `Model.new.save`: `.new` resolves through the project's own
/// zero-arg `initialize` (`Bucket::Resolved`, isolating this fixture from
/// the `initialize`-is-also-in-`AR_API_INSTANCE` case exercised
/// (`Ty::Instance` receiver) is real `ActiveRecord::Base` instance API
/// (`activerecord_api.txt` line 358) and must escalate.
#[test]
fn instance_level_api_escalates_silently() {
    let diags = check_fixture("instance_level_silent.rb");
    assert!(diags.is_empty(), "`.save` is real ActiveRecord::Base instance API: expected silence, got {diags:?}");

    let s = stats_fixture("instance_level_silent.rb");
    assert_eq!(s.ar_api_method, 1, "`.save` must be tallied as the curated-inventory hit: {s:?}");
    assert_eq!(s.resolved, 1, "`.new` resolves through the project's own `initialize`: {s:?}");
    assert_eq!(s.inconclusive, 0, "a resolved hit must not also count as inconclusive: {s:?}");
    assert_eq!(s.total(), 2, "two call sites in the fixture (`.new`, `.save`): {s:?}");
}

/// The single most important test in this file (see the module header's
/// mutant 1): `.custom_widget_flag` sits on a declared-external
/// `ActiveRecord::Base` ancestor but is in NEITHER `AR_API_INSTANCE` nor
/// `AR_API_SINGLETON` — a generated attribute/association or client
/// method no static declaration can name. The escalation MUST miss, and
/// the call MUST stay `Inconclusive` (silent, never E0101) rather than
/// either being falsely resolved OR falsely diagnosed.
#[test]
fn name_outside_inventory_never_manufactures_diagnostic() {
    let diags = check_fixture("neither_set_silent.rb");
    assert!(
        diags.is_empty(),
        "a name outside both AR API sets must stay Inconclusive, never a manufactured E0101: got {diags:?}"
    );

    let s = stats_fixture("neither_set_silent.rb");
    assert_eq!(s.ar_api_method, 0, "the curated inventory must miss `custom_widget_flag`: {s:?}");
    assert_eq!(s.not_ar_api, 1, "the miss must land in the unreachable-remainder bucket: {s:?}");
    assert_eq!(s.resolved, 1, "`.new` resolves through the project's own `initialize`: {s:?}");
    assert_eq!(s.inconclusive, 1, "`.custom_widget_flag` stays blind, exactly as before this bead: {s:?}");
}

/// A project model that reopens a real `ActiveRecord::Base` API name
/// (`save`) with its own definition: normal lookup (`lookup_method_rbi`)
/// finds it directly on the receiver's own class, BEFORE `rbi_escalate`
/// is ever reached (`rbi_escalate` is only called from the
/// `MethodLookup::Inconclusive` arm) — the project's own method always
/// wins. Verified two ways: `definition_at` resolves to the fixture's own
/// `def save`, AND `call_stats` shows the call never touched
/// `ar_api_method`. Both matter — `definition_at` runs a SILENT walk, and
/// `rbi_escalate` short-circuits on `self.silent` before it ever reaches
/// the reordering mutant 4 targets, so `definition_at` alone cannot catch
/// that specific mutation; only the `ar_api_method` stats assertion does
/// (see the module header's mutant 4 for the confirmed transcript).
#[test]
fn project_definition_wins_over_curated_inventory() {
    let (path, text) = fixture_text("own_definition_wins.rb");
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, path.clone().into(), text.clone());
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);

    let diags = itaruby_semantic::check_file(&db, file);
    assert!(diags.is_empty(), "expected silence, got {diags:?}");

    let offset = offset_in(&text, "ArApiOwnDefinitionWins.new.save", "save");
    let site = itaruby_semantic::definition_at(&db, file, offset)
        .expect("`.save` must resolve to the project's own `def save`, not the curated inventory");
    let li = itaruby_semantic::LineIndex::new(&text);
    let (line, _) = li.line_col(&text, site.start);
    assert_eq!(line + 1, 5, "`def save` is on line 5 of the fixture");

    let s = itaruby_semantic::call_stats(&db, file);
    assert_eq!(s.ar_api_method, 0, "no call site here goes through the curated inventory: {s:?}");
    assert_eq!(s.resolved, 2, "both `.new` and `.save` resolve against the project's own defs: {s:?}");
}

/// Control: a class with NO `ActiveRecord` ancestor at all calling a name
/// that happens to be in the curated inventory. `ProjectIndex::
/// declared_by(id, "ActiveRecord::Base")` is false (no such ancestor
/// exists), so `rbi_escalate`'s third step never even reaches the name
/// check — behavior is byte-identical to before this bead: a fully closed
/// ancestry with no matching method is a real `NotFound`, diagnosed.
#[test]
fn no_ar_ancestor_no_escalation() {
    let diags = check_fixture("no_ancestor_unchanged.rb");
    assert_eq!(diags.len(), 1, "expected exactly one diagnostic, got {diags:?}");
    assert!(diags[0].contains("E0101"), "expected E0101, got {:?}", diags[0]);
    assert!(diags[0].contains("save"), "message should name the undefined method, got {:?}", diags[0]);

    let s = stats_fixture("no_ancestor_unchanged.rb");
    assert_eq!(s.ar_api_method, 0, "no declared-external ActiveRecord::Base ancestor: escalation must not fire: {s:?}");
    assert_eq!(s.diagnosed, 1, "`.save` on a fully closed ancestry is a real E0101: {s:?}");
}
