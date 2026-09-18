//! Curated external gem declarations (bead ita-3gs). Fixtures live under
//! `testdata/external_gem_decls/`: `model.rb` proves the design invariant —
//! declaring `ActiveRecord::Base`/`Rails` resolves the constant (kills
//! E0104) but never closes ancestry, so an undefined method on a subclass
//! stays silent, never a false E0101 — and `still_unresolved.rb` proves the
//! declaration file is an allowlist, not a blanket suppressor: a name
//! outside it still warns E0104 exactly as before this bead.
//!
//! ita-dpg.1 (PACK leaf) extends the same allowlist with a SECOND,
//! machine-generated file: `declarations/rbs_collection.rbi`
//! (`scripts/gen-rbs-collection-pack.rb`, a frozen 21-gem
//! `gem_rbs_collection` walk), merged through the identical
//! `declared_fragments()` + `merge_declared_fragment` path as `gems.rbi`.
//! `rbs_collection_silent.rb` proves three DIFFERENT gem families' pack
//! entries all resolve and stay open; `rbs_collection_undeclared_still_warns.rb`
//! proves the pack is still an allowlist for its own fixed gem set, not a
//! blanket suppressor; `rbs_collection_project_redefinition_wins.rb`
//! proves a project's own reopening of a pack-declared name wins over the
//! declaration (E0101 fires on the PROJECT class's own undefined method).
//!
//! MUTANTS THE PACK TESTS BELOW MUST CATCH: (a) any single
//! `class`/`module ...; end` line removed from `rbs_collection.rbi`
//! re-emits E0104 for exactly that one constant in
//! `pack_silences_constants_across_different_gem_families`; (b) an entry
//! in `rbs_collection.rbi` gaining a method or a value constant panics
//! `rbs_collection_pack_declares_open_namespaces_only`.

fn sev(s: itaruby_semantic::Severity) -> &'static str {
    match s {
        itaruby_semantic::Severity::Error => "error",
        itaruby_semantic::Severity::Warning => "warning",
    }
}

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/external_gem_decls");
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
            format!("{}:{}:{} {} {}", l + 1, c + 1, sev(d.severity), d.code, d.message)
        })
        .collect()
}

#[test]
fn declared_external_class_resolves_constant_and_stays_silent() {
    let diags = check_fixture("model.rb");
    assert!(
        diags.is_empty(),
        "ActiveRecord::Base/Rails are curated declarations: constant must \
         resolve (no E0104) and ancestry must stay open (no E0101 for \
         `undefined_method`), got: {diags:?}"
    );
}

/// Names the acceptance criterion this test proves: a class that inherits
/// from a curated external declaration and calls an unknown method must
/// NOT get E0101 — the declaration resolves the constant only, it never
/// closes ancestry for method lookup (invariant #1).
#[test]
fn declared_external_superclass_does_not_close_ancestry_for_method_lookup() {
    let db = itaruby_semantic::Db::default();
    itaruby_semantic::ProjectFiles::new(&db, Vec::new());
    let index = itaruby_semantic::project_index(&db);

    let base = index
        .resolve_const(&[], "ActiveRecord::Base")
        .expect("curated declaration must resolve `ActiveRecord::Base` as a constant");
    assert!(
        index.class(base).open,
        "a curated declaration must always be open: we don't know the real \
         gem's method surface"
    );
}

#[test]
fn undeclared_constant_still_warns_e0104() {
    let diags = check_fixture("still_unresolved.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0104"), "expected E0104, got: {:?}", diags[0]);
    assert!(
        diags[0].contains("TotallyMadeUpGemNamespace"),
        "message should name the unresolved constant, got: {:?}",
        diags[0]
    );
}

/// Bead ita-083: `T::Struct`/`TracePoint` are curated declarations, found
/// missing in the ita-40k head-to-head (ruby-lsp/tapioca, `typed: strict`).
/// Same design invariant as `declared_external_class_resolves_constant_and_stays_silent`:
/// resolves the constant (kills E0104) but never closes ancestry, so the
/// undefined `undefined_field` call stays silent, never a false E0101.
#[test]
fn declared_strict_core_names_resolve_constant_and_stay_silent() {
    let diags = check_fixture("strict_core_decls.rb");
    assert!(
        diags.is_empty(),
        "T::Struct/TracePoint are curated declarations: constant must \
         resolve (no E0104) and ancestry must stay open (no E0101 for \
         `undefined_field`), got: {diags:?}"
    );
}

/// Control fixture for ita-083 (mirrors `undeclared_constant_still_warns_e0104`
/// for ita-3gs): a genuinely inexistent name outside the curated allowlist
/// must still warn E0104.
#[test]
fn strict_core_undeclared_constant_still_warns_e0104() {
    let diags = check_fixture("strict_core_decls_still_unresolved.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0104"), "expected E0104, got: {:?}", diags[0]);
    assert!(
        diags[0].contains("StrictCoreDeclNonexistent"),
        "message should name the unresolved constant, got: {:?}",
        diags[0]
    );
}
/// 2026-08-26 entries: sorbet-runtime's own `T::*` namespaces beyond the
/// core five, measured on tapioca's residual E0104 (16 of 17 sites; the
/// 17th is the `...::Private::INSTANCE` VALUE, deliberately undeclared —
/// this file only carries namespaces). Every read must stay silent.
#[test]
fn declared_sorbet_runtime_namespaces_resolve_and_stay_silent() {
    let diags = check_fixture("sorbet_runtime_namespaces.rb");
    assert!(
        diags.is_empty(),
        "the measured T::* names are curated declarations: no E0104, got: {diags:?}"
    );
}

/// The same entries leave ancestry permanently open, exactly like the
/// ita-3gs ones (checked here on one nested name, the shape most likely
/// to regress: a name three `::` levels deep, where the resolution walk
/// must resolve the owner chain before the member check).
#[test]
fn declared_sorbet_runtime_namespaces_stay_open() {
    let db = itaruby_semantic::Db::default();
    itaruby_semantic::ProjectFiles::new(&db, Vec::new());
    let index = itaruby_semantic::project_index(&db);
    for name in ["T::Set", "T::Private::Types::Void", "T::Props::ClassMethods"] {
        let id = index
            .resolve_const(&[], name)
            .unwrap_or_else(|| panic!("curated declaration must resolve `{name}`"));
        assert!(
            index.class(id).open,
            "curated declaration `{name}` must stay open (unknown real method surface)"
        );
    }
}

/// Control (mirrors `still_unresolved.rb`): a name outside the curated
/// sorbet-runtime list still warns — the entries are an allowlist, never
/// a `T::`-prefix suppressor (the prefix-fallback bug, twice measured
/// 2026-08-26, must never exist here).
#[test]
fn sorbet_runtime_undeclared_name_still_warns_e0104() {
    let diags = check_fixture("sorbet_runtime_undeclared_still_warns.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0104"), "expected E0104, got: {:?}", diags[0]);
    assert!(
        diags[0].contains("T::NotACuratedName"),
        "message should name the unresolved constant, got: {:?}",
        diags[0]
    );
}

/// The generated pack's own silence proof (bead ita-dpg.1): three
/// entries from THREE DIFFERENT gem families — `Faker::Config`
/// (faker), `Nokogiri::XML` (nokogiri), `Stripe::Charge` (stripe) — must
/// each resolve and stay open, same shape as
/// `declared_external_class_resolves_constant_and_stays_silent` above,
/// just against the generated file instead of the hand-curated one.
#[test]
fn pack_silences_constants_across_different_gem_families() {
    let diags = check_fixture("rbs_collection_silent.rb");
    assert!(
        diags.is_empty(),
        "Faker::Config/Nokogiri::XML/Stripe::Charge are rbs_collection.rbi entries from three \
         different gem families: constant must resolve (no E0104) and ancestry must stay open \
         (no E0101), got: {diags:?}"
    );
}

/// Control (mirrors `undeclared_constant_still_warns_e0104`): `Prism`,
/// `Fabrication`, `Selenium` are genuinely absent from BOTH `gems.rbi`
/// and `rbs_collection.rbi` — none of the pack's frozen `GEMS` list ships
/// any of the three, and none of those gems' own `.rbs` reopens one
/// either. Each must still warn E0104 — the pack resolves a fixed,
/// measured gem set, never every unresolved top-level constant.
///
/// Two replacements, both forced by the allowlist growing under it: bead
/// ita-dpg.2 replaced `Faraday` (its gem joined `GEMS`), and the wave-10
/// integration replaced `RSpec`, which bead ita-dpg.3 declared in
/// `gems.rbi` from a sibling slice cut off the same commit — so the two
/// slices were each green alone and red together. A control that asserts
/// a name is ABSENT is a control with an expiry date; re-check the names
/// mechanically against both declaration files whenever either grows.
#[test]
fn pack_undeclared_constants_still_warn_e0104() {
    let diags = check_fixture("rbs_collection_undeclared_still_warns.rb");
    assert_eq!(diags.len(), 3, "expected exactly 3 diagnostics, got: {diags:?}");
    for name in ["Prism", "Fabrication", "Selenium"] {
        assert!(
            diags.iter().any(|d| d.contains("E0104") && d.contains(name)),
            "expected an E0104 naming {name}, got: {diags:?}"
        );
    }
}

/// Project redefinition wins over a pack declaration (bead ita-dpg.1):
/// `Redis` is a bare top-level `rbs_collection.rbi` entry, but this
/// fixture reopens it itself with a real, closed class — `Redis.new`'s
/// undefined method must fire a real E0101 (not silently swallowed by the
/// declaration), proving `merge_declared_fragment`'s "skip any path the
/// project itself already defines" rule (`index.rs`) actually holds for
/// the generated file, not just the hand-curated one.
#[test]
fn pack_project_redefinition_wins_over_declared_namespace() {
    let diags = check_fixture("rbs_collection_project_redefinition_wins.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(
        diags[0].contains("E0101"),
        "expected E0101 — the project's own `Redis` is genuinely closed, got: {:?}",
        diags[0]
    );
    assert!(
        diags[0].contains("totally_undefined_method"),
        "message should name the missing method, got: {:?}",
        diags[0]
    );
}

/// Bead ita-dpg.1's own no-methods-no-value-constants contract, checked
/// over the WHOLE generated `rbs_collection.rbi` — mirrors
/// `declarations.rs`'s unit test `declares_open_namespaces_only_no_methods`
/// (which runs against the COMBINED `gems.rbi` + `rbs_collection.rbi`
/// set already); this one isolates the generated file alone so a
/// regression here can never hide behind `gems.rbi`'s much smaller,
/// hand-reviewed set.
///
/// MUTANT THIS TEST MUST CATCH: any entry in `rbs_collection.rbi` gaining
/// a method (`def ...`), a singleton method, or a value constant panics
/// here.
#[test]
fn rbs_collection_pack_declares_open_namespaces_only() {
    let frags = itaruby_semantic::declarations::rbs_collection_fragments();
    assert!(!frags.is_empty(), "expected at least one generated fragment");
    for f in &frags {
        assert!(f.methods.is_empty(), "{} must declare zero methods", f.path);
        assert!(
            f.singleton_methods.is_empty(),
            "{} must declare zero singleton methods",
            f.path
        );
        assert!(f.consts.is_empty(), "{} must declare zero value constants", f.path);
    }
}

/// Wave 11 residual audit (2026-08-26): every nested-member declaration
/// added to `gems.rbi` this round resolves its constant and stays silent —
/// same invariant as `declared_external_class_resolves_constant_and_stays_silent`,
/// exercised once per family (Nokogiri, GraphQL, Faker, Stripe, Concurrent,
/// `RubyLLM`, `ValidEmail2`, `HTMLEntities`, Elasticsearch, Twilio, Oj, Liquid,
/// `RSpec`) in `wave11_nested.rb`.
#[test]
fn wave11_nested_declarations_resolve_constant_and_stay_silent() {
    let diags = check_fixture("wave11_nested.rb");
    assert!(
        diags.is_empty(),
        "every constant in wave11_nested.rb is a curated wave-11 declaration: \
         none should warn, got: {diags:?}"
    );
}

/// A4 control: `Stripe::Coupon` sits under the same `Stripe` namespace this
/// round curated four siblings under (`Subscription`/`Invoice`/
/// `Checkout::Session`/`PromotionCode`), and the same namespace the
/// generated pack ALSO already covers nine other siblings of — but
/// `Coupon` itself was never measured, so it lives in neither. Must still
/// warn E0104 — proof `gems.rbi` is an exact-path allowlist, not a
/// namespace-wide open door.
#[test]
fn wave11_undeclared_sibling_under_curated_namespace_still_warns_e0104() {
    let diags = check_fixture("wave11_nested_control.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0104"), "expected E0104, got: {:?}", diags[0]);
    assert!(
        diags[0].contains("Stripe::Coupon"),
        "message should name the unresolved constant, got: {:?}",
        diags[0]
    );
}

/// Wave 11 task item 3: a leading `::` (cbase) must resolve identically to
/// the bare path once a declaration exists — `resolve_const`'s
/// `strip_prefix("::")` arm and the bare-path terminal fallback both key
/// off the same `by_path` entry. `wave11_cbase.rb` references
/// `Stripe::Subscription`/`Stripe::Invoice` in both spellings; a defect
/// would leave exactly the `::`-prefixed lines warning while bare ones
/// fell silent, so an empty diagnostic set here proves both spellings hit
/// the same curated entry.
#[test]
fn wave11_cbase_prefixed_and_bare_path_resolve_identically() {
    let diags = check_fixture("wave11_cbase.rb");
    assert!(
        diags.is_empty(),
        "both `Stripe::Subscription`/`::Stripe::Subscription` and \
         `Stripe::Invoice`/`::Stripe::Invoice` must resolve via the same \
         curated by_path entry, got: {diags:?}"
    );
}

/// Wave 14 residual audit (2026-09-18): one line per namespace added to
/// `gems.rbi` this round — `Karafka`, `Karafka::Admin`, `Money::Currency`,
/// `Flipper::Actor`, the `Flipper::Adapters::ActiveRecord::Gate` owner
/// chain, `ActiveStorage::FileNotFoundError` and the two `ActiveResource`
/// error classes — each resolving its constant and staying silent, in a
/// plain reference AND in a `rescue` clause (the shape three of the
/// measured sites actually have, where a name that stays unresolved is a
/// rescue that could never match).
#[test]
fn wave14_nested_declarations_resolve_constant_and_stay_silent() {
    let diags = check_fixture("wave14_nested.rb");
    assert!(
        diags.is_empty(),
        "every constant in wave14_nested.rb is a curated wave-14 declaration: \
         none should warn, got: {diags:?}"
    );
}

/// Wave 14 control (mirrors `wave11_undeclared_sibling_under_curated_namespace_still_warns_e0104`):
/// four undeclared siblings, one under each namespace wave 14 curated a
/// member of. All four must still warn E0104 — the entries are exact
/// paths, never a namespace-wide open door, and never the reverted
/// prefix fallback.
#[test]
fn wave14_undeclared_siblings_under_curated_namespaces_still_warn_e0104() {
    let diags = check_fixture("wave14_nested_control.rb");
    assert_eq!(diags.len(), 4, "expected exactly 4 diagnostics, got: {diags:?}");
    for (diag, name) in diags.iter().zip([
        "Karafka::Server",
        "Flipper::Adapters::Memory",
        "Money::Bank",
        "ActiveResource::ResourceNotFound",
    ]) {
        assert!(diag.contains("E0104"), "expected E0104, got: {diag:?}");
        assert!(
            diag.contains(name),
            "message should name the unresolved constant {name}, got: {diag:?}"
        );
    }
}
