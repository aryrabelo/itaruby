//! W3 require/autoload: stdlib constants gated on the project's own
//! literal `require` calls (`index.rs::stdlib_declares`, harvest embedded
//! at `declarations/stdlib_constants.txt`). Fixtures live in
//! `testdata/std_requires/`: `silenced.rb` proves the gate fires (require
//! present → constant resolves), `no_require_still_warns.rb` and
//! `unknown_lib_still_warns.rb` prove the two control sides — the same
//! constant with the require absent, and a require naming a lib the
//! harvest does not know.

fn sev(s: itaruby_semantic::Severity) -> &'static str {
    match s {
        itaruby_semantic::Severity::Error => "error",
        itaruby_semantic::Severity::Warning => "warning",
    }
}

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/std_requires");
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
fn stdlib_constant_with_require_silences_e0104() {
    let diags = check_fixture("silenced.rb");
    assert!(
        !diags.iter().any(|d| d.contains("E0104")),
        "require 'json' is present, so JSON and JSON::ParserError must resolve; got: {diags:?}"
    );
    assert!(
        diags.is_empty(),
        "the fixture must be fully silent, got: {diags:?}"
    );
}

#[test]
fn same_stdlib_constant_without_require_keeps_warning() {
    let diags = check_fixture("no_require_still_warns.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0104"), "expected E0104, got: {:?}", diags[0]);
    assert!(
        diags[0].contains("ERB"),
        "message should name the unresolved constant, got: {:?}",
        diags[0]
    );
}

#[test]
fn require_of_unknown_lib_unlocks_nothing() {
    let diags = check_fixture("unknown_lib_still_warns.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0104"), "expected E0104, got: {:?}", diags[0]);
}

/// The require gate is project-wide (Ruby's `require` is process-global)
/// but per-lib: a require in ONE file resolves the constant in a DIFFERENT
/// file of the same project, and a require of a DIFFERENT lib unlocks
/// nothing.
#[test]
fn require_is_project_wide_but_per_lib() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/std_requires");
    let db = itaruby_semantic::Db::default();
    let a_path = format!("{dir}/silenced.rb");
    let b_path = format!("{dir}/no_require_still_warns.rb");
    let a_text = std::fs::read_to_string(&a_path).unwrap();
    let b_text = std::fs::read_to_string(&b_path).unwrap();
    let a = itaruby_semantic::SourceFile::new(&db, a_path.into(), a_text);
    let b = itaruby_semantic::SourceFile::new(&db, b_path.into(), b_text);
    itaruby_semantic::ProjectFiles::new(&db, vec![a, b]);
    // `ERB` in file b still warns: 'erb' is required nowhere in THIS
    // project — file a only required 'json'.
    let diags: Vec<String> = itaruby_semantic::check_file(&db, b)
        .iter()
        .map(|d| d.code.to_string())
        .collect();
    assert_eq!(
        diags,
        vec!["E0104"],
        "a require of a DIFFERENT lib must not unlock ERB: {diags:?}"
    );
}

/// Scope-qualified candidate: a bare stdlib const referenced inside an
/// app-reopened `module JSON` resolves via the cref-qualified spelling
/// (`JSON::ParserError`) even though the bare name never appears in the
/// harvest.
#[test]
fn bare_stdlib_const_inside_reopened_namespace_resolves() {
    let db = itaruby_semantic::Db::default();
    let text = "require 'json'\nmodule JSON\n  class StdReqReopener\n    def m(x)\n      ParserError\n    end\n  end\nend\n";
    let file = itaruby_semantic::SourceFile::new(&db, "w3_scope.rb".into(), text.to_string());
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    let diags: Vec<String> = itaruby_semantic::check_file(&db, file)
        .iter()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect();
    assert!(
        diags.is_empty(),
        "bare ParserError inside module JSON resolves as JSON::ParserError: {diags:?}"
    );
}
