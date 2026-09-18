//! `db/schema.rb`-derived model attributes (bead ita-yho). All fixtures
//! live under `testdata/schema_models/`, sharing one toy `db/schema.rb`;
//! every scenario is loaded into the same multi-file `ProjectFiles` so
//! cross-file schema resolution runs exactly as it does under `ita check`.

fn sev(s: itaruby_semantic::Severity) -> &'static str {
    match s {
        itaruby_semantic::Severity::Error => "error",
        itaruby_semantic::Severity::Warning => "warning",
    }
}

/// Checks one model fixture against the shared toy schema. All files under
/// `testdata/schema_models/` are registered in the same project (schema.rb
/// plus every model fixture) so class reopening across files never skews a
/// single scenario's result; diagnostics are collected for `model_file`
/// only. E0104 (`unresolved constant ApplicationRecord`) is filtered out:
/// it's orthogonal, pre-existing noise — `ApplicationRecord` genuinely has
/// no project definition here, exactly like `ExternalBase` in
/// `testdata/definition_silent.rb`, and this bead's whole point is that
/// that ancestry gap must stay open (see
/// `nonexistent_attribute_stays_inconclusive_not_e0101`), not get patched
/// over by defining a fake `ApplicationRecord` in the fixtures.
fn check_with_schema(model_file: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/schema_models");

    let mut sources = Vec::new();
    let db = itaruby_semantic::Db::default();
    let mut target = None;
    for entry in std::fs::read_dir(format!("{dir}/db"))
        .expect("read db dir")
        .chain(std::fs::read_dir(dir).expect("read fixtures dir"))
    {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rb") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read fixture file");
        let file = itaruby_semantic::SourceFile::new(&db, path.clone(), text.clone());
        if path.file_name().and_then(|n| n.to_str()) == Some(model_file) {
            target = Some((file, text));
        }
        sources.push(file);
    }
    itaruby_semantic::ProjectFiles::new(&db, sources);

    let (file, text) = target.unwrap_or_else(|| panic!("fixture {model_file} not found under {dir}"));
    let li = itaruby_semantic::LineIndex::new(&text);
    itaruby_semantic::check_file(&db, file)
        .iter()
        .filter(|d| d.code != "E0104")
        .map(|d| {
            let (l, c) = li.line_col(&text, d.start);
            format!("{}:{}:{} {} {}", l + 1, c + 1, d.code, sev(d.severity), d.message)
        })
        .collect()
}

#[test]
fn valid_attr_is_silent() {
    let diags = check_with_schema("valid_attr.rb");
    assert!(diags.is_empty(), "known columns with castable literals must stay silent, got: {diags:?}");
}

#[test]
fn non_numeric_string_into_integer_warns_e0106() {
    let diags = check_with_schema("bad_numeric_literal.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0106"), "expected E0106, got: {:?}", diags[0]);
    assert!(diags[0].contains("warning"), "E0106 must be a Warning, never an Error: {:?}", diags[0]);
    assert!(diags[0].contains("quantity"), "message should name the column, got: {:?}", diags[0]);
    assert!(diags[0].contains("integer"), "message should name the column type, got: {:?}", diags[0]);
}

#[test]
fn numeric_string_into_integer_is_silent() {
    // The case that separates real checking from noise: Rails' cast
    // recovers `"42"` into `42`, so this must never fire.
    let diags = check_with_schema("numeric_string_ok.rb");
    assert!(diags.is_empty(), "a numeric string literal must cast fine and stay silent, got: {diags:?}");
}

#[test]
fn digitless_string_into_datetime_warns_e0106() {
    let diags = check_with_schema("bad_temporal_literal.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0106"), "expected E0106, got: {:?}", diags[0]);
    assert!(diags[0].contains("launched_at"), "message should name the column, got: {:?}", diags[0]);
}

#[test]
fn nonexistent_attribute_stays_inconclusive_not_e0101() {
    // The single most important assertion in this file: the model inherits
    // from an unresolved external base (`ApplicationRecord`), so an unknown
    // attribute must resolve `Inconclusive`, exactly like a plain unknown
    // method on any other externally-based class (see
    // `testdata/definition_silent.rb`) — never `NotFound`/E0101.
    let diags = check_with_schema("unknown_attr.rb");
    assert!(
        diags.is_empty(),
        "unknown attribute on an ApplicationRecord model must never become E0101 (ancestry stays \
         incomplete), got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.contains("E0101")),
        "explicit check: no E0101 anywhere for this fixture, got: {diags:?}"
    );
}

#[test]
fn unknown_typed_variable_into_integer_is_silent() {
    let diags = check_with_schema("unknown_var_numeric.rb");
    assert!(
        diags.is_empty(),
        "E0106 is literal-argument-only; a variable of unknown static type must stay silent, got: \
         {diags:?}"
    );
}

#[test]
fn literal_table_name_resolves_and_still_checks() {
    // Presence of E0106 here is the positive proof that `self.table_name =
    // "sprockets"` actually took effect: `Gizmo` conventionally pluralizes
    // to `gizmos` (absent from the schema), so this diagnostic can only
    // fire if the literal override won.
    let diags = check_with_schema("table_name_override.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0106"), "expected E0106, got: {:?}", diags[0]);
    assert!(diags[0].contains("count"), "message should name the column, got: {:?}", diags[0]);
}

#[test]
fn model_without_a_matching_table_is_silent() {
    let diags = check_with_schema("missing_table.rb");
    assert!(
        diags.is_empty(),
        "a class whose derived/declared table isn't in the schema must stay silent, got: {diags:?}"
    );
}
