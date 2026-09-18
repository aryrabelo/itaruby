//! Bead ita-oaq: a project-code reopening of a gem's own base class — the
//! common `module Minitest; class Test; include ProjectHelper; end; end`
//! pattern used to mix project test helpers into Minitest's base test class
//! — sits BETWEEN the call site and the gem's real ancestry. Before this
//! bead, `index.rs::gem_reopens` (the `soften_not_found` "gap 1" fallback)
//! checked only the RECEIVER class's own path against the client's Tapioca
//! `rbi_map`, never any ancestor reached through a fully-resolved
//! include/superclass chain. `Minitest::Test` resolves as a genuine, CLOSED
//! project ancestor two hops up from a real test class (`FooTest < TestCase
//! < Minitest::Test`), so the chain looked fully closed and every Minitest
//! DSL method (`assert_equal`, ...) the project itself never redeclares
//! reported a fabricated E0101 — measured at 1650/1650 E0101 on ruby-lsp's
//! own test suite, 1625 of them exactly this shape.
//!
//! Fixtures live in `testdata/rbi_mixin/` (prefix `RbiMix`, gate c's
//! globally-unique-prefix rule) so the repro is human-inspectable with a
//! plain `ita check testdata/rbi_mixin` — same convention as
//! `testdata/rbi_ancestry/`. The RBI is wired EXPLICITLY here, never via
//! filesystem discovery (same reason `rbi_ancestry.rs`'s `check_with_rbi`
//! does: `ita check testdata/` scans `testdata/` as ONE root, and per-root
//! discovery never reaches a `sorbet/rbi` nested one level below the root
//! it's given — gate c's plain run of `testdata/rbi_mixin/class_test.rb`
//! therefore still warns today, on purpose, exactly like
//! `rbi_ancestry/probe.rb` does).

use std::path::PathBuf;

fn fixture_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/rbi_mixin"))
}

/// Loads every `.rb` fixture file (NOT `.rbi`) into one `ProjectFiles`
/// project, optionally wires the fixture's `sorbet/rbi` explicitly, and
/// returns the diagnostics for `target` alone (the same "load the whole
/// project, report one file" shape `ita check` uses, minus filesystem RBI
/// discovery).
fn check_with_rbi(target: &str, wire_rbi: bool) -> Vec<(String, String)> {
    let dir = fixture_dir();
    let mut rb_files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|ext| ext == "rb"))
        .collect();
    rb_files.sort();
    assert!(!rb_files.is_empty(), "fixture dir must carry .rb files");

    let db = itaruby_semantic::Db::default();
    let mut target_file = None;
    let files: Vec<_> = rb_files
        .iter()
        .map(|path| {
            let text = std::fs::read_to_string(path).unwrap();
            let file = itaruby_semantic::SourceFile::new(&db, path.clone(), text);
            if path.file_name().unwrap() == target {
                target_file = Some(file);
            }
            file
        })
        .collect();
    itaruby_semantic::ProjectFiles::new(&db, files);

    if wire_rbi {
        let rbi_dir = dir.join("sorbet/rbi");
        let rbi_paths = itaruby_semantic::rbi::discover_rbi_files(&rbi_dir);
        let index = itaruby_semantic::rbi::build_rbi_index(&rbi_paths);
        assert!(!index.constants.is_empty(), "fixture RBI must index");
        itaruby_semantic::RbiProject::new(&db, index.constants);
    }

    itaruby_semantic::check_file(&db, target_file.expect("target file must be among fixtures"))
        .iter()
        .map(|d| (d.code.to_string(), d.message.clone()))
        .collect()
}

/// The headline case: `RbiMixClassTest < RbiMixTestCase < Minitest::Test`
/// (the project's own reopening of `Minitest::Test`, mixing in
/// `RbiMixTestHelper`) — a Minitest DSL method (`assert_equal`) declared
/// only on the REAL gem's `Minitest::Assertions` module must resolve
/// silently once the RBI is wired, even though `Minitest::Test` sits TWO
/// hops above the call site, not on the receiver itself.
#[test]
fn mixin_two_hops_up_resolves_with_rbi_wired() {
    let diags = check_with_rbi("class_test.rb", true);
    assert!(
        diags.is_empty(),
        "assert_equal lives on Minitest::Assertions, mixed into Minitest::Test two hops above \
         the receiver via the project's own reopening; expected silence, got: {diags:?}"
    );
}

/// Same fixture, RBI absent: the fabricated E0101 this bead fixes — proves
/// the fixture genuinely reproduces the bug shape (matches gate c's own
/// plain-run behavior, see this file's header) rather than being silent by
/// construction.
#[test]
fn without_rbi_wired_the_same_call_site_is_a_fabricated_e0101() {
    let diags = check_with_rbi("class_test.rb", false);
    assert_eq!(diags.len(), 1, "expected exactly the fabricated E0101, got: {diags:?}");
    assert_eq!(diags[0].0, "E0101", "expected E0101, got: {:?}", diags[0]);
    assert!(
        diags[0].1.contains("assert_equal"),
        "expected the assert_equal miss, got: {:?}",
        diags[0].1
    );
}

/// Control: a project class with NO relation whatsoever to `Minitest::Test`
/// (no ancestor chain overlap with anything the RBI declares) calling a
/// method that genuinely does not exist anywhere must keep reporting
/// E0101 even with the same RBI wired in the same project — proves the fix
/// is scoped to the receiver's OWN resolved ancestor chain, never a
/// blanket "some `rbi_map` exists somewhere" silence.
#[test]
fn unrelated_class_with_a_real_miss_still_warns_with_rbi_wired() {
    let diags = check_with_rbi("control_test.rb", true);
    assert_eq!(diags.len(), 1, "expected exactly the real miss, got: {diags:?}");
    assert_eq!(diags[0].0, "E0101", "expected E0101, got: {:?}", diags[0]);
    assert!(
        diags[0].1.contains("totally_undefined_method_xyz"),
        "expected the real miss, got: {:?}",
        diags[0].1
    );
}

/// Direct-query mutant guard, independent of the checker plumbing (same
/// "direct query" pattern `rbi_methods.rs` uses): `gem_reopens` must walk
/// the WHOLE ancestor chain, not just `id` itself. Reverting to `id`-only
/// (the pre-ita-oaq shape) makes `lookup_method_rbi` return `NotFound` for
/// the two-hops-up receiver — the exact regression this bead fixes.
#[test]
fn gem_reopens_must_see_past_the_receiver_to_the_ancestor_that_matches_rbi() {
    let dir = fixture_dir();
    let db = itaruby_semantic::Db::default();
    let files: Vec<_> = ["reopen_helper.rb", "test_case.rb", "class_test.rb"]
        .iter()
        .map(|name| {
            let path = dir.join(name);
            let text = std::fs::read_to_string(&path).unwrap();
            itaruby_semantic::SourceFile::new(&db, path, text)
        })
        .collect();
    itaruby_semantic::ProjectFiles::new(&db, files);
    let index = itaruby_semantic::project_index(&db);

    let rbi_dir = dir.join("sorbet/rbi");
    let rbi_files = itaruby_semantic::rbi::discover_rbi_files(&rbi_dir);
    let rbi_map = itaruby_semantic::rbi::build_rbi_index(&rbi_files).constants;

    let receiver = *index
        .by_path
        .get("RbiMixClassTest")
        .expect("RbiMixClassTest must be interned");
    assert!(
        !rbi_map.contains_key("RbiMixClassTest"),
        "the receiver's OWN path must not be the thing the RBI declares — \
         otherwise this test would pass even with the id-only pre-fix behavior"
    );
    match index.lookup_method_rbi(receiver, "assert_equal", Some(&rbi_map)) {
        itaruby_semantic::index::MethodLookup::NotFound => {
            panic!(
                "gem_reopens regressed to checking only the receiver's own path: \
                 Minitest::Test, two ancestors up, is what the RBI actually declares"
            );
        }
        itaruby_semantic::index::MethodLookup::Inconclusive => {
            // Correct: `Minitest::Test` (an ancestor of `RbiMixClassTest`,
            // not the receiver itself) is a gem reopening, so the miss
            // must escalate rather than conclude `NotFound`.
        }
        itaruby_semantic::index::MethodLookup::Found(..) => {
            panic!("this fixture's project index never defines assert_equal itself");
        }
    }
}
