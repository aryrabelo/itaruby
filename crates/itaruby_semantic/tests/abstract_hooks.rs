//! A self-send dispatches on the RUNTIME class, so a method missing from a
//! subclassed class may be supplied by a descendant — claiming `NotFound`
//! there is a false positive, which violates invariant #1.
//!
//! Found by review of one private benchmark corpus (corpus-a, 2026-08-20), not by us: three of
//! the six baseline "verified true positives" were wrong about the code. An
//! abstract base's template method self-sends three hooks, all three defined
//! by its single subclass — which is the only class ever instantiated, so
//! every one of those sends resolves at runtime. Precision was 3/6, and the
//! checker's own entry bar ("zero false positives") was false.
//!
//! Fixtures under `testdata/abstract_hooks/`. Three cases, because a
//! suppression rule proven on one side only is half a measurement: the hook
//! supplied by a descendant must go silent, and BOTH the subclassed-but-
//! nobody-defines-it case and the leaf case must still accuse.

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/abstract_hooks");
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

/// The false positive. `AbsHookChild` supplies `abs_hook_host`, so the
/// base's self-send resolves at runtime.
#[test]
fn hook_supplied_by_descendant_is_silent() {
    let diags = check_fixture("subclass_supplies_hook.rb");
    assert!(
        diags.is_empty(),
        "`abs_hook_host` is defined by the only subclass: a self-send \
         dispatches on the runtime class, so no E0101. Got: {diags:?}"
    );
}

/// The other half. Being subclassed must not silence a method that no
/// descendant defines either — otherwise the rule would have suppressed the
/// corpus's other real error too, which lives in exactly that shape.
#[test]
fn subclassed_but_no_descendant_defines_it_still_reports() {
    let diags = check_fixture("no_descendant_defines_hook.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0101"), "expected E0101, got: {:?}", diags[0]);
    assert!(
        diags[0].contains("abs_hook_missing"),
        "message should name the missing method, got: {:?}",
        diags[0]
    );
}

/// Precision on a leaf class is untouched: no descendants, no ambiguity.
#[test]
fn leaf_class_still_reports() {
    let diags = check_fixture("leaf_class_still_reports.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0101"), "expected E0101, got: {:?}", diags[0]);
    assert!(
        diags[0].contains("abs_hook_absent"),
        "message should name the missing method, got: {:?}",
        diags[0]
    );
}
