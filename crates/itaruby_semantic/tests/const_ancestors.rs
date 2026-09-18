//! Constant lookup follows the ancestor chain, not only lexical nesting.
//! Measured on one private benchmark corpus (corpus-a, 2026-08-20): 14 distinct constants —
//! 28 warnings — were reported unresolved while being defined right there
//! in the same tree, in three real shapes: a constant from an `include`d
//! sibling module, a constant defined on the superclass, and a `Struct.new`
//! assigned to a constant on the superclass. Fixtures under
//! `testdata/const_ancestors/`: three positives that must be silent, one
//! negative control that must still accuse — a suppression rule proven on
//! one side only is half a measurement.

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/const_ancestors");
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
            format!("{}:{}:{} {}", l + 1, c + 1, d.code, d.message)
        })
        .collect()
}

#[test]
fn constant_from_included_module_resolves() {
    let diags = check_fixture("included_module_const.rb");
    assert!(
        diags.is_empty(),
        "`CONST_ANC_LIMIT` comes from the included `ConstAncShared`: no E0104, got: {diags:?}"
    );
}

#[test]
fn constant_from_superclass_resolves() {
    let diags = check_fixture("superclass_const.rb");
    assert!(
        diags.is_empty(),
        "`CONST_ANC_PATTERN` is defined on the superclass: no E0104, got: {diags:?}"
    );
}

#[test]
fn nested_class_from_superclass_resolves() {
    let diags = check_fixture("inherited_nested_class.rb");
    assert!(
        diags.is_empty(),
        "`ConstAncInner` is nested in the superclass: no E0104, got: {diags:?}"
    );
}

/// The other half: widening lookup must not degrade into "the name exists
/// somewhere in the project". A sibling module that is never included stays
/// unreachable.
#[test]
fn constant_on_unrelated_sibling_module_still_warns() {
    let diags = check_fixture("unrelated_module_const.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0104"), "expected E0104, got: {:?}", diags[0]);
    assert!(
        diags[0].contains("CONST_ANC_SECRET"),
        "message should name the unresolved constant, got: {:?}",
        diags[0]
    );
}
