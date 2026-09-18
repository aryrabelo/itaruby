//! Bead ita-c8h: `apply_undeclared_namespace_reopenings` (`index.rs`) —
//! generalizes `apply_gem_reopenings` (ita-547) past `Gemfile.lock` gem
//! names entirely. Round-5 audit: mastodon's `config/initializers/
//! rack_attack.rb` reopens `class Rack::Attack::Request` (7 sites), whose
//! REAL superclass — `Rack::Request`, carrying `ip`/`path`/`params` — is
//! declared only inside the `rack` gem. `apply_gem_reopenings` happens to
//! already close that specific case (mastodon's lock separately names the
//! `rack` gem, and the plain-camelize guess for `rack` is `Rack`, matching
//! the reopening's own top segment) — but that coverage is coincidental,
//! not structural: nothing requires a hyphenated gem's implied namespace
//! segment to also be its own separately-locked gem, and a project with no
//! `Gemfile.lock` at all gets zero coverage from that mechanism. These
//! fixtures carry no `Gemfile.lock`, proving the NEW mechanism covers the
//! shape on its own.
//!
//! Fixtures live in `testdata/gem_superclass_reopen/`, prefixed
//! `GemSuperReopen` for global uniqueness across `testdata/`.

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/gem_superclass_reopen");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap();
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, path.into(), text);
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    itaruby_semantic::check_file(&db, file)
        .iter()
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect()
}

/// G1: reopening a class nested under a namespace this project never
/// itself wraps bare (`class GemSuperReopenGemNs::Request`, no
/// `Gemfile.lock` in sight) stays silent on a method it never defines —
/// the exact `Rack::Attack::Request` shape from the Round-5 audit.
#[test]
fn nested_gem_reopen_with_undeclared_namespace_stays_silent() {
    let diags = check_fixture("silent_nested_gem_reopen.rb");
    assert!(diags.is_empty(), "expected silence, got {diags:?}");
}

/// G2 (negative control): a bare top-level project class — no `::` in
/// its path — keeps accusing exactly as before. The mechanism only ever
/// widens silence for a nested path.
#[test]
fn bare_project_class_still_warns() {
    let diags = check_fixture("control_bare_still_warns.rb");
    assert_eq!(diags.len(), 1, "expected exactly one diagnostic, got {diags:?}");
    assert!(diags[0].starts_with("E0101"), "expected E0101, got {diags:?}");
}

/// Negative control: the parent namespace IS defined by the project
/// itself (a bare `module GemSuperReopenApp; end`, the idiomatic
/// Rails-generator shape) — a nested reopening under it must NOT be
/// mistaken for an external gem's namespace, and a genuine missing
/// method keeps accusing.
#[test]
fn nested_class_under_project_defined_namespace_still_warns() {
    let diags = check_fixture("control_parent_defined_still_warns.rb");
    assert_eq!(diags.len(), 1, "expected exactly one diagnostic, got {diags:?}");
    assert!(diags[0].starts_with("E0101"), "expected E0101, got {diags:?}");
}
