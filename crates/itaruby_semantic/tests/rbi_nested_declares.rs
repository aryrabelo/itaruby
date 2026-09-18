//! Bead ita-y0s: nested-block RBI declarations invisible to phase-1
//! lookup. Phase 1 (`rbi.rs::scan_class_name`) trims leading whitespace
//! from every `class`/`module` header line it scans, so a header NESTED
//! inside another block in a `.rbi` file (`module Origem; module Coisa;
//! ...; end; end`) registers `rbi_map` under its BARE last simple name
//! ("Coisa") — never the fully qualified path (`Origem::Coisa`) phase 2's
//! real prism parse (`index.rs::rbi_file_fragments`, tracking genuine
//! nesting) computes for that same fragment. `rbi_declares`'s and
//! `rbi_qualified_const_declares`'s owner lookup both did a plain
//! `rbi_map.get(name)` — an exact-key miss on the full qualified name
//! died before phase 2 was ever consulted, even though phase 2 would
//! have answered correctly the moment it was reached.
//!
//! Fix (`crates/itaruby_semantic/src/index.rs`): new `rbi_map_candidates`
//! helper — on an exact-key miss, retry under the name's bare last
//! `::`-segment, returning whatever candidates that key carries.
//! `rbi_declares` and `rbi_qualified_const_declares`'s owner lookup both
//! call it instead of a raw `rbi_map.get`. The fallback only ever WIDENS
//! which files get a chance to answer each call site's existing per-file
//! refilter (`f.path == name` / `f.path == owner`, both exact-string
//! comparisons against phase 2's real parse) — it never widens what
//! counts as a match, so a bare-name collision between two unrelated
//! namespaces still degrades to a silent miss, never a first-file hit.
//!
//! Fixtures under `testdata/rbi_nested_declares/` mirror the three
//! shapes of the original local scratch cases `{a,b,c}` exactly (renamed
//! with a globally unique `RbiY0sCase{A,B,C}` prefix per AGENTS.md/bead
//! ita-u1t):
//! case a is real project nesting (no RBI at all, already silent, must
//! stay silent); case b is the bug (nested `.rbi` blocks, was a false
//! E0104, must go silent); case c is the compact `.rbi` form (already
//! silent via the exact-match fast path, must stay silent).
//! `homonym_control.rb` plus `rbi_y0s_homonym_{x,y}.rbi` are the
//! MANDATORY anti-regression control: two different `.rbi` files nest a
//! module under the identical bare simple name.
//!
//! MUTANTS THIS FILE MUST CATCH:
//!   (a) the bare-last-segment fallback removed from `rbi_map_candidates`
//!       (e.g. it degenerates back to a plain `rbi_map.get(name)`) ->
//!       `case_b_nested_rbi_reference_resolves_silently` fails: BOTH the
//!       alias RHS's direct reference and the member reached through the
//!       alias re-emit the exact false E0104 this bead exists to kill.
//!   (b) the per-file refilter removed from a call site so ANY candidate
//!       file returned by the bare-name fallback counts as a hit,
//!       regardless of whether its own real fragment path/qualified-write
//!       owner actually matches (e.g. `rbi_qualified_const_declares`
//!       short-circuiting `true` the moment `rbi_map_candidates(owner,
//!       ..)` returns `Some`, without checking `f.path == owner`) ->
//!       `homonym_cross_file_member_still_accuses` wrongly goes silent —
//!       a member that belongs to the OTHER file under the same bare key
//!       leaks a suppression it never earned.
//!
//! Negative controls (`case_a_project_nesting_stays_silent`,
//! `case_c_compact_rbi_stays_silent`, `homonym_own_member_resolves`) all
//! pass unconditionally today and MUST keep passing after the fix — they
//! prove the fallback neither breaks the pre-existing exact-match path
//! nor breaks ordinary project-only resolution.

fn dir() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/rbi_nested_declares")
}

/// Loads every named fixture into one project plus this dir's own toy
/// `sorbet/rbi`, mirroring `tests/const_alias.rs`'s
/// `check_project_with_rbi` harness. Returns each file's diagnostics, in
/// the same order as `names`, formatted as `line:col CODE message`.
fn check_project(names: &[&str]) -> Vec<Vec<String>> {
    let dir = dir();
    let db = itaruby_semantic::Db::default();
    let texts: Vec<String> = names
        .iter()
        .map(|name| std::fs::read_to_string(format!("{dir}/{name}")).unwrap())
        .collect();
    let files: Vec<itaruby_semantic::SourceFile> = names
        .iter()
        .zip(texts.iter())
        .map(|(name, text)| {
            itaruby_semantic::SourceFile::new(&db, format!("{dir}/{name}").into(), text.clone())
        })
        .collect();
    itaruby_semantic::ProjectFiles::new(&db, files.clone());

    let rbi_dir = std::path::Path::new(dir).join("sorbet/rbi");
    let rbi_files = itaruby_semantic::rbi::discover_rbi_files(&rbi_dir);
    assert!(!rbi_files.is_empty(), "fixture must carry at least one .rbi file");
    let index = itaruby_semantic::rbi::build_rbi_index(&rbi_files);
    itaruby_semantic::RbiProject::new(&db, index.constants);

    files
        .iter()
        .zip(texts.iter())
        .map(|(file, text)| {
            let li = itaruby_semantic::LineIndex::new(text);
            itaruby_semantic::check_file(&db, *file)
                .iter()
                .map(|d| {
                    let (l, c) = li.line_col(text, d.start);
                    format!("{}:{}:{} {}", l + 1, c + 1, d.code, d.message)
                })
                .collect()
        })
        .collect()
}

fn check_single(name: &str) -> Vec<String> {
    check_project(&[name]).into_iter().next().unwrap()
}

/// Negative control (must pass before AND after this bead): real project
/// nesting, no RBI involved at all.
#[test]
fn case_a_project_nesting_stays_silent() {
    let diags = check_single("case_a_project_nesting_silent.rb");
    assert!(diags.is_empty(), "case a must stay silent, got: {diags:?}");
}

/// Mutant (a): the actual bug. Nested `.rbi` blocks — both the alias
/// RHS's direct reference to `RbiY0sCaseBOrigem::Coisa` and
/// `Atalho::VALOR` reached through the alias must resolve silently.
#[test]
fn case_b_nested_rbi_reference_resolves_silently() {
    let diags = check_single("case_b_nested_rbi_resolves_silently.rb");
    assert!(
        diags.is_empty(),
        "case b (nested .rbi module blocks) must resolve silently through the bare-name \
         fallback + phase-2 refilter, got: {diags:?}"
    );
}

/// Negative control (must pass before AND after this bead): compact-form
/// `.rbi` header already registers the full qualified name exactly.
#[test]
fn case_c_compact_rbi_stays_silent() {
    let diags = check_single("case_c_compact_rbi_resolves_silently.rb");
    assert!(
        diags.is_empty(),
        "case c (compact .rbi form) must stay silent via the pre-existing exact-match path, \
         got: {diags:?}"
    );
}

/// Negative control: the query's own genuine owner (`RbiY0sHomonymOuterX::Shared`)
/// resolves through the bare-name fallback exactly like case b.
#[test]
fn homonym_own_member_resolves() {
    let diags = check_single("homonym_control.rb");
    assert_eq!(
        diags.len(),
        1,
        "expected exactly 1 diagnostic (only the cross-file member should accuse), got: {diags:?}"
    );
}

/// Mutant (b): the mandatory anti-regression control. A member that
/// belongs ONLY to the unrelated `RbiY0sHomonymOuterY::Shared` fragment,
/// reached through the SAME bare-name fallback key ("Shared") as the
/// genuine `RbiY0sHomonymOuterX::Shared` owner, must keep accusing — the
/// fallback hands the query BOTH candidate files, and only the per-file
/// exact-path refilter tells them apart.
#[test]
fn homonym_cross_file_member_still_accuses() {
    let diags = check_single("homonym_control.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(
        diags[0].contains("E0104") && diags[0].contains("Y_ONLY"),
        "a member belonging only to the homonym's OTHER file must still accuse — the bare-name \
         fallback must never resolve to \"the first file under that key\", got: {diags:?}"
    );
}
