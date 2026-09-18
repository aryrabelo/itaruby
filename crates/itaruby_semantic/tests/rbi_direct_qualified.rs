//! Bead ita-3dg: `check.rs::check_const_ref`'s DIRECT branch never tried
//! `index.rs::rbi_qualified_const_declares` on `path` as written — only
//! `alias_rbi_leaf_declares` did, and only when `path` is ITSELF an
//! unresolved literal alias (`ProjectIndex::expand_unresolved_alias_target`
//! requires a `find_const_alias` hit on the failing segment). A qualified
//! (`RuboCop::Version::STRING = T.let(T.unsafe(nil), String)` in
//! `sorbet/rbi/gems/rubocop@*.rbi`, `RuboCop::Version` never a project
//! class/module) — has no alias for `expand_unresolved_alias_target` to
//! chase, so it always returned `None`, and only the bare `rbi_declares`
//! (class/module header match) ran, which a value write can never
//! satisfy: E0104 fired even though the constant is genuinely declared,
//! right there, in the client's own vendorized RBI.
//!
//! Fix (`crates/itaruby_semantic/src/check.rs`): `check_const_ref`'s
//! `rbi_map` block also tries `rbi_qualified_const_declares(path, map)`
//! directly, beside the existing `rbi_declares(path, map)` — same
//! suppression-only contract (see that function's own doc comment): a
//! hit only silences E0104, `infer_const` still types the reference
//! `Ty::Unknown` (invariant #1).
//!
//! Fixtures live under `testdata/rbi_direct_qualified/`: the vendored
//! gem's own toy RBI (`sorbet/rbi/gems/rbi_3dg_vendored_gem.rbi`)
//! declares `module Rbi3dgVendoredGem::Version` plus the separate
//! toplevel qualified write
//! `Rbi3dgVendoredGem::Version::STRING = T.let(...)` — the real
//! `qualified_writes` shape, never nested inside the module body.
//! `direct_ref_resolves.rb`'s `read` is a DIRECT reference (no alias
//! anywhere); its own `bad` and the separate
//! `control_undeclared_owner.rb` are the anti-suppression controls.
//!
//! MUTANTS THIS FILE MUST CATCH:
//!   (a) the new `crate::index::rbi_qualified_const_declares(path, map)`
//!       call removed from `check_const_ref`'s `rbi_map` block ->
//!       `direct_qualified_ref_to_rbi_const_write_resolves_silently`
//!       fails: `read`'s reference re-emits the exact false E0104 this
//!       bead exists to kill.
//!   (b) `rbi_qualified_const_declares`'s real per-name check weakened to
//!       "the owner exists in `rbi_map` at all" (e.g. dropping the
//!       `qualified_writes`/`consts` membership test and returning
//!       `true` whenever `rbi_map.get(owner)` is `Some`) ->
//!       `same_owner_undeclared_member_still_accuses` and
//!       `undeclared_owner_still_accuses` both wrongly go silent — the
//!       fix must only ever suppress a member the RBI's own writes
//!       genuinely name, never every member under a declared owner.

fn sev(s: itaruby_semantic::Severity) -> &'static str {
    match s {
        itaruby_semantic::Severity::Error => "error",
        itaruby_semantic::Severity::Warning => "warning",
    }
}

fn fixture_dir() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/rbi_direct_qualified")
}

/// Loads one fixture file plus the fixture's own toy `sorbet/rbi`,
/// mirroring `tests/rbi.rs`'s `check_fixture` harness.
fn check_fixture(name: &str) -> Vec<String> {
    let dir = fixture_dir();
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap();
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, path.into(), text.clone());
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);

    let rbi_dir = std::path::Path::new(dir).join("sorbet/rbi");
    let files = itaruby_semantic::rbi::discover_rbi_files(&rbi_dir);
    assert!(!files.is_empty(), "fixture must carry at least one .rbi file");
    let index = itaruby_semantic::rbi::build_rbi_index(&files);
    itaruby_semantic::RbiProject::new(&db, index.constants);

    let li = itaruby_semantic::LineIndex::new(&text);
    itaruby_semantic::check_file(&db, file)
        .iter()
        .map(|d| {
            let (l, c) = li.line_col(&text, d.start);
            format!("{}:{}:{} {} {}", l + 1, c + 1, sev(d.severity), d.code, d.message)
        })
        .collect()
}

/// Mutant (a): `read`'s direct qualified reference (no alias anywhere)
/// resolves silently through the vendored RBI's qualified const-write;
/// `bad`'s genuinely undeclared `NOPE` under the SAME owner still
/// accuses (mutant (b)'s first half).
#[test]
fn direct_qualified_ref_to_rbi_const_write_resolves_silently() {
    let diags = check_fixture("direct_ref_resolves.rb");
    assert_eq!(
        diags.len(),
        1,
        "expected exactly 1 diagnostic (only `bad` should accuse), got: {diags:?}"
    );
    assert!(
        diags[0].contains("E0104") && diags[0].contains("NOPE"),
        "`read` must resolve silently through the vendorized RBI's qualified const-write \
         (`STRING` is declared there); `bad`'s genuinely undeclared `NOPE` must still accuse, \
         got: {diags:?}"
    );
}

/// Mutant (b), second half: an owner that is declared nowhere at all
/// (not project, not RBI) must still warn — a wider negative control
/// than `bad` above, which shares its owner with a real declaration.
#[test]
fn undeclared_owner_still_accuses() {
    let diags = check_fixture("control_undeclared_owner.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(
        diags[0].contains("E0104") && diags[0].contains("Rbi3dgNeverDeclaredAnywhere"),
        "a qualified reference whose owner is declared nowhere must keep accusing, got: {diags:?}"
    );
}
