//! Bead ita-hzd: RBI fallback never walks lexical nesting. Tapioca/Sorbet
//! writes a vendored gem's `class`/`module` header at its full qualified
//! path (`class RbiHzdRubyLsp::Notification < ::RbiHzdRubyLsp::Message`), never
//! inferring a project's own lexical nesting — but a project file can
//! still legitimately reference that constant BARE from deep inside a
//! matching nesting (`module RbiHzdRubyLsp; module Tapioca; class Addon; ...;
//! Notification; ...; end; end; end`, exactly ruby-lsp's own
//! `lib/ruby_lsp/tapioca/addon.rb` shape). Before this bead,
//! `check.rs::check_const_ref`'s RBI channels
//! (`rbi_declares`/`rbi_qualified_const_declares`/`rbi_ancestor_declares`)
//! only ever tried `path` AS WRITTEN ("Notification"), never the
//! nesting-expanded form the project's own `const_exists`/`resolve_const`
//! walk would try for an ordinary project constant — so this bare
//! reference fired a false E0104 even though the RBI plainly declares it,
//! three lexical levels up.
//!
//! Fix (`crates/itaruby_semantic/src/check.rs`): new
//! `Checker::nesting_expanded_rbi_declares`, tried only after `path` as
//! written already missed every existing RBI channel — walks `scope`
//! innermost-to-outermost (mirrors `ProjectIndex::const_exists`'s own
//! bare-name walk), retrying `rbi_declares`/`rbi_qualified_const_declares`/
//! `rbi_ancestor_declares` against each `<level>::path` candidate TEXT.
//! Every channel keeps its own exact-string refilter unchanged — this
//! only widens which full path gets tried, never how loosely a match
//! counts. Suppression-only, same contract as every other RBI channel
//! (invariant #1): a hit only silences E0104, `infer_const` still types
//! the reference `Ty::Unknown`.
//!
//! Fixtures under `testdata/rbi_nesting_fallback/`:
//! `sorbet/rbi/gems/rbi_hzd_vendored_gem.rbi` declares
//! `RbiHzdRubyLsp::Notification` (compact form) and, for the mutant (b) guard,
//! `RbiHzdRubyLsp::NotificationHandlerExtra` (a name that merely has
//! `NotificationHandler` as a string PREFIX, never itself declared).
//!
//! MUTANTS THIS FILE MUST CATCH:
//!   (a) `nesting_expanded_rbi_declares` disabled/removed (e.g.
//!       `check_const_ref`'s `|| self.nesting_expanded_rbi_declares(...)`
//!       arm dropped) -> `bare_ref_resolves_through_nesting_walk` fails,
//!       re-emitting the exact false E0104 this bead exists to kill.
//!   (b) the walk widened to accept a prefix/substring match instead of
//!       each RBI channel's own exact-string equality (e.g. checking
//!       `rbi_map.keys().any(|k| k.starts_with(&candidate))` instead of
//!       an exact `rbi_declares`/`rbi_qualified_const_declares` call) ->
//!       `prefix_collision_still_accuses` wrongly goes silent: the RBI's
//!       `RbiHzdRubyLsp::NotificationHandlerExtra` would falsely answer a bare
//!       `NotificationHandler` reference it was never declared for.
//!
//! Negative control (`undeclared_at_any_nesting_level_still_accuses`): a
//! name genuinely absent from the RBI at every nesting level this
//! reference's scope could ever walk must keep accusing.

fn dir() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/rbi_nesting_fallback")
}

/// Loads one fixture file plus this dir's own toy `sorbet/rbi`, mirroring
/// `tests/rbi.rs`'s `check_fixture` harness.
fn check_fixture(name: &str) -> Vec<String> {
    let dir = dir();
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
            format!("{}:{}:{} {}", l + 1, c + 1, d.code, d.message)
        })
        .collect()
}

/// Mutant (a): the actual bug. `handle`'s bare `Notification`, 3 levels
/// deep inside `RbiHzdRubyLsp::Tapioca::Addon`, must resolve through the
/// nesting-expanded walk finding `RbiHzdRubyLsp::Notification`. `bad`'s
/// genuinely undeclared constant (also the negative control below) must
/// keep accusing.
#[test]
fn bare_ref_resolves_through_nesting_walk() {
    let diags = check_fixture("nesting_bare_ref_resolves.rb");
    assert_eq!(
        diags.len(),
        1,
        "expected exactly 1 diagnostic (only `bad` should accuse), got: {diags:?}"
    );
    assert!(
        diags[0].contains("E0104") && diags[0].contains("NeverDeclaredAtAnyNestingLevel"),
        "`handle`'s bare `Notification` must resolve through the nesting-expanded RBI walk \
         (`RbiHzdRubyLsp::Notification` is declared there); `bad`'s genuinely undeclared constant \
         must still accuse, got: {diags:?}"
    );
}

/// Sanity re-check of the same fixture's negative half in isolation.
#[test]
fn undeclared_at_any_nesting_level_still_accuses() {
    let diags = check_fixture("nesting_bare_ref_resolves.rb");
    assert!(
        diags
            .iter()
            .any(|d| d.contains("E0104") && d.contains("NeverDeclaredAtAnyNestingLevel")),
        "a constant absent from the RBI at every nesting level must still accuse, got: {diags:?}"
    );
}

/// Mutant (b): a name that is only a string PREFIX of a real RBI
/// declaration, at every nesting-expanded candidate, must never resolve.
#[test]
fn prefix_collision_still_accuses() {
    let diags = check_fixture("control_no_partial_match.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(
        diags[0].contains("E0104") && diags[0].contains("NotificationHandler"),
        "`NotificationHandler` must never resolve merely because \
         `RbiHzdRubyLsp::NotificationHandlerExtra` shares it as a string prefix — none of the nesting \
         candidates spell an RBI declaration EXACTLY, got: {diags:?}"
    );
}
