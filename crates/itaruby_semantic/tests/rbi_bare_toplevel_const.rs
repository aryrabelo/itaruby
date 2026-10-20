//! Bead ita-h5s: a bare, unqualified top-level value-constant write in a
//! project's `sorbet/rbi/**` file (`XClient = T.let(T.unsafe(nil),
//! T.nilable(Integer))`, no `::` anywhere in the LHS — the real
//! planningalerts shape, `sorbet/rbi/shims/elasticsearch_client.rbi`'s
//! `ElasticSearchClient = T.let(T.unsafe(nil),
//! T.nilable(Elasticsearch::Client))`) was never read by any RBI
//! channel before this bead: `rbi_declares` only matches a `class`/
//! `module` header, and `rbi_qualified_const_declares` requires
//! `qualified.rsplit_once("::")` to succeed, returning `false`
//! immediately for a name with no `::` at all. Confirmed by the
//! monotonicity report's minimal probe (`v1-asis` vs `v3-noshim`:
//! removing the shim changed nothing — it was never consulted).
//!
//! Fix: `rbi.rs::scan_toplevel_const_write` (phase 1) unions bare
//! value-constant-write candidates into the SAME shared `rbi_map`
//! `rbi_declares`/`rbi_qualified_const_declares` already use;
//! `index.rs::rbi_toplevel_const_declares` (phase 2) verifies via a real
//! Prism parse (`FileDefs::toplevel_consts`, or a `consts` entry nested
//! directly in a `class Object`/`module Kernel` fragment) and is wired
//! into `check.rs::Checker::rbi_fallback_declares` beside the other two
//! direct channels.
//!
//! Fixtures live under `testdata/rbi_bare_toplevel_const/`:
//! `sorbet/rbi/shims/x_client.rbi` declares the bare `XClient` write
//! plus a `DESC = <<~TEXT ... FakeConst = 1 ... TEXT` heredoc whose BODY
//! line is a phase-1 false-positive candidate (`FakeConst`) that phase
//! 2's real parse must reject.
//!
//! MUTANTS THIS FILE MUST CATCH:
//!   (a) `rbi_toplevel_const_declares` never wired into
//!       `rbi_fallback_declares` -> `bare_toplevel_const_resolves_silently`
//!       fails: `read`'s reference re-emits the exact false E0104 this
//!       bead exists to kill.
//!   (b) `scan_toplevel_const_write` dropped from (or never unioned
//!       into) `build_rbi_index` -> same failure as (a), proving the
//!       phase-1 candidate step is load-bearing too.
//!   (c) `rbi_toplevel_const_declares` weakened to "the candidate file
//!       exists" (dropping the `toplevel_consts`/`Object`/`Kernel`
//!       membership check) -> `heredoc_body_false_positive_still_accuses`
//!       wrongly goes silent on `FakeConst`, which only ever appears as
//!       a heredoc BODY line, never a real constant write.
//!   (d) a hit wrongly interned into `ProjectIndex` (violating
//!       suppression-only, invariant #1) ->
//!       `method_call_on_bare_const_stays_unknown` wrongly emits E0101
//!       for a nonexistent method call.
//!   (e) `undeclared` or `UndeclaredConst` wrongly silenced (this
//!       channel over-widened to match everything) ->
//!       `undeclared_bare_const_still_accuses` fails.

fn sev(s: itaruby_semantic::Severity) -> &'static str {
    match s {
        itaruby_semantic::Severity::Error => "error",
        itaruby_semantic::Severity::Warning => "warning",
    }
}

fn fixture_dir() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/rbi_bare_toplevel_const")
}

/// Loads one fixture file plus the fixture's own toy `sorbet/rbi`,
/// mirroring `tests/rbi_direct_qualified.rs`'s `check_fixture` harness.
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

/// Mutants (a)/(b): `read`'s bare reference to `XClient` resolves
/// silently through the vendored RBI's bare top-level value-constant
/// write; `undeclared`'s genuinely undeclared `UndeclaredConst` still
/// accuses (mutant (e)).
#[test]
fn bare_toplevel_const_resolves_silently() {
    let diags = check_fixture("bare_const_resolves.rb");
    assert_eq!(
        diags.len(),
        1,
        "expected exactly 1 diagnostic (only `undeclared` should accuse), got: {diags:?}"
    );
    assert!(
        diags[0].contains("E0104") && diags[0].contains("UndeclaredConst"),
        "`read` must resolve silently through the vendorized RBI's bare top-level write \
         (`XClient` is declared there); `undeclared`'s genuinely undeclared `UndeclaredConst` \
         must still accuse, got: {diags:?}"
    );
}

/// Mutant (e), wider control: a bare name declared nowhere at all (not
/// project, not RBI) must still warn.
#[test]
fn undeclared_bare_const_still_accuses() {
    let diags = check_fixture("bare_const_resolves.rb");
    assert!(
        diags.iter().any(|d| d.contains("E0104") && d.contains("UndeclaredConst")),
        "got: {diags:?}"
    );
}

/// Mutant (c): `FakeConst` only ever appears as a heredoc BODY line in
/// the vendored RBI — a real phase-1/phase-2 split proof, not just a
/// comment (which phase 1's own scanner already rejects by construction,
/// since a `#`-prefixed line never starts with an uppercase letter).
#[test]
fn heredoc_body_false_positive_still_accuses() {
    let diags = check_fixture("control_heredoc_body_const_still_accuses.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(
        diags[0].contains("E0104") && diags[0].contains("FakeConst"),
        "a constant that only ever appears as a heredoc BODY line (never a real \
         ConstantWriteNode) must still accuse — phase 2's real Prism parse must reject the \
         phase-1 false-positive candidate, got: {diags:?}"
    );
}

/// Mutant (d): the RBI hit must stay suppression-only — `Ty::Unknown`,
/// never interned into `ProjectIndex` — so a nonexistent method call on
/// the resolved constant never manufactures a new E0101.
#[test]
fn method_call_on_bare_const_stays_unknown() {
    let diags = check_fixture("method_call_stays_unknown.rb");
    assert!(
        diags.is_empty(),
        "a bare RBI-declared constant must stay Ty::Unknown; an arbitrary method call on it \
         must never emit E0101 (invariant #1), got: {diags:?}"
    );
}
