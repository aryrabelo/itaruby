//! Bead ita-547: generalizes mechanism B (bead ita-h6l,
//! `index.rs::is_known_external_class_path`) past its curated
//! core/stdlib/`gems.rbi` lists — a gem this project's own `Gemfile.lock`
//! declares but that curated set never covers (measured, Round-5 audit:
//! `mail`, `wikicloth`, `fastimage`) is mapped to a guessed Ruby namespace
//! (`discovery.rs::gem_namespace`) and, when a project reopening's
//! top-level path segment matches that guess exactly, forced open — the
//! reopening no longer looks like a complete project class definition.
//!
//! Fixtures live in `testdata/gem_reopen_lockfile/`, each checked in its
//! OWN single-file project with `wire_declaration_sources` run for real
//! against that directory (so the fixture `Gemfile.lock` sitting directly
//! in it is discovered exactly the way `ita check`/`ita server` discover
//! one, never hand-constructed).

use std::path::PathBuf;

fn fixture_dir() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/gem_reopen_lockfile"
    ))
}

/// Checks one fixture file in its own project, with `wire_declaration_sources`
/// run against the fixture directory (real upward `Gemfile.lock` discovery,
/// not a hand-wired singleton) — the end-to-end path `ita check`/`ita server`
/// both go through.
fn check_fixture(name: &str) -> Vec<String> {
    let dir = fixture_dir();
    let path = dir.join(name);
    let text = std::fs::read_to_string(&path).unwrap();
    let mut db = itaruby_semantic::Db::default();
    itaruby_semantic::wire_declaration_sources(&mut db, &[dir]);
    let file = itaruby_semantic::SourceFile::new(&db, path, text);
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    itaruby_semantic::check_file(&db, file)
        .iter()
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect()
}

/// G1: an initializer reopening `Mail::SMTP` (not in the curated
/// `gems.rbi` allowlist) and calling a method this file never defines
/// stays silent — `mail` is declared in the fixture's `Gemfile.lock`.
#[test]
fn mail_reopen_matching_lockfile_gem_stays_silent() {
    let diags = check_fixture("silent_mail_reopen.rb");
    assert!(diags.is_empty(), "expected silence, got {diags:?}");
}

/// Capitalization the segment boundaries do not predict: `rspec` in the
/// lock, `RSpec` in the code. This is the defect `gem_namespace_key`
/// fixed on 2026-09-17 — the camelize guess `Rspec` never equalled
/// `RSpec`, so the reopening looked like a complete project definition
/// and a typo on a REAL gem class method (`RSpec.descrybe`) was a
/// candidate accusation. Measured on the reference corpora by reopening
/// site before the fix: mastodon `ConnectionPool` (2), discourse
/// `MessageBus` (1).
#[test]
fn rspec_reopen_matching_lockfile_gem_case_insensitively_stays_silent() {
    let diags = check_fixture("silent_rspec_reopen.rb");
    assert!(diags.is_empty(), "expected silence, got {diags:?}");
}

/// The only exception kind the key cannot absorb: a namespace differing
/// in LETTERS. `kt-paperclip` 8.0.0 defines `module Paperclip`
/// (`lib/paperclip.rb:82`, read in the gem's own source at the version
/// mastodon locks).
#[test]
fn kt_paperclip_reopen_needs_the_exception_table_and_stays_silent() {
    let diags = check_fixture("silent_kt_paperclip_reopen.rb");
    assert!(diags.is_empty(), "expected silence, got {diags:?}");
}

/// The generalization this mechanism REFUSES, pinned: `elasticsearch-api`
/// is in the lock and `api` is one of its hyphen segments, but `Api` is
/// the project's own namespace and stays checked. Matching by segment
/// would have blinded 145 reopening sites under `Api` in mastodon, 16
/// under `Auth` in discourse (`auth-sanitizer`), plus `Scheduler`,
/// `Form`, `Event` and `Web` (measured 2026-09-17).
#[test]
fn gem_name_segment_never_opens_a_project_namespace() {
    let diags = check_fixture("control_gem_name_segment_still_warns.rb");
    assert_eq!(diags.len(), 1, "expected exactly one diagnostic, got {diags:?}");
    assert!(diags[0].starts_with("E0101"), "expected E0101, got {diags:?}");
}

/// `wikicloth` -> `WikiCloth` and `fastimage` -> `FastImage`: two entries
/// that used to need the override table and now fall out of
/// `gem_namespace_key` for free. Kept as tests because the BEHAVIOR is
/// the contract, not the mechanism that delivers it.
#[test]
fn internal_capital_gems_stay_silent_without_an_override_entry() {
    for name in ["silent_wikicloth_reopen.rb", "silent_fastimage_reopen.rb"] {
        let diags = check_fixture(name);
        assert!(diags.is_empty(), "expected silence for {name}, got {diags:?}");
    }
}

/// G2 (negative control): a project-owned class unrelated to any gem in
/// the lock keeps accusing E0101 exactly as before — the mechanism never
/// blankets project code, only classes under a MAPPED gem namespace.
#[test]
fn own_class_unrelated_to_lockfile_still_warns() {
    let diags = check_fixture("control_own_class_still_warns.rb");
    assert_eq!(diags.len(), 1, "expected exactly one diagnostic, got {diags:?}");
    assert!(diags[0].starts_with("E0101"), "expected E0101, got {diags:?}");
}

/// `apply_gem_reopenings` (ita-547) alone would miss this: `activesupport`
/// IS in the lock, but the plain-camelize guess (`Activesupport`) does not
/// match the real `ActiveSupport` spelling, so no override covers it
/// either — an exact top-level match alone never widens here. Bead
/// ita-c8h's complementary `apply_undeclared_namespace_reopenings` closes
/// the gap independent of any gem-name guess (this fixture never wraps
/// `ActiveSupport` bare), so the two mechanisms together stay silent. See
/// `gem_superclass_reopen.rs` for fixtures isolating ita-c8h's own
/// mechanism with no `Gemfile.lock` involved at all.
#[test]
fn unmapped_gem_reopen_still_covered_by_undeclared_namespace_fallback() {
    let diags = check_fixture("control_unmapped_gem_still_warns.rb");
    assert!(diags.is_empty(), "expected silence, got {diags:?}");
}
