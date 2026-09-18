//! One test per diagnostic code, against fixtures in `testdata/`. Asserts
//! on codes and message substrings, not full snapshot strings (columns can
//! shift; only the E0101 line number is pinned exactly).

fn sev(s: itaruby_semantic::Severity) -> &'static str {
    match s {
        itaruby_semantic::Severity::Error => "error",
        itaruby_semantic::Severity::Warning => "warning",
    }
}

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata");
    let text = std::fs::read_to_string(format!("{dir}/{name}")).unwrap();
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, format!("{dir}/{name}").into(), text.clone());
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    let li = itaruby_semantic::LineIndex::new(&text);
    itaruby_semantic::check_file(&db, file)
        .iter()
        .map(|d| {
            let (l, c) = li.line_col(&text, d.start);
            format!("{}:{}:{} {} {}", l + 1, c + 1, d.code, sev(d.severity), d.message)
        })
        .collect()
}

#[test]
fn unknown_method() {
    let diags = check_fixture("unknown_method.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    let d = &diags[0];
    assert!(d.starts_with("7:"), "expected diagnostic on line 7, got: {d}");
    assert!(d.contains("E0101"), "expected E0101, got: {d}");
    assert!(d.contains("nmae"), "message should name the typo `nmae`, got: {d}");
    assert!(d.contains("User"), "message should name the receiver class `User`, got: {d}");
}

#[test]
fn arity() {
    let diags = check_fixture("arity.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    let d = &diags[0];
    assert!(d.contains("E0102"), "expected E0102, got: {d}");
    assert!(d.contains("expects 2"), "message should say expects 2, got: {d}");
    assert!(d.contains("got 3"), "message should say got 3, got: {d}");
}

#[test]
fn rbs_sig() {
    let diags = check_fixture("rbs_sig.rb");
    assert_eq!(
        diags.len(),
        1,
        "expected exactly 1 diagnostic (the literal-string mismatch; the \
         Unknown-typed call must stay silent), got: {diags:?}"
    );
    assert!(diags[0].contains("E0103"), "expected E0103, got: {:?}", diags[0]);
}

#[test]
fn open_class() {
    let diags = check_fixture("open_class.rb");
    assert!(
        diags.is_empty(),
        "method_missing makes the class open: no diagnostics allowed, got: {diags:?}"
    );
}

#[test]
fn reopen() {
    let diags = check_fixture("reopen.rb");
    assert!(
        diags.is_empty(),
        "method defined in a later reopened fragment must still resolve, got: {diags:?}"
    );
}

#[test]
fn syntax_err() {
    let diags = check_fixture("syntax_err.rb");
    assert!(
        diags.iter().any(|d| d.contains("E0001")),
        "expected at least one E0001 syntax error, got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.contains("E0101")),
        "a broken file must not fabricate unknown-method diagnostics, got: {diags:?}"
    );
}
