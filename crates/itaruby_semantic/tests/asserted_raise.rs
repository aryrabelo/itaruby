//! Bead ita-w2c, bead E (the owner's NARROW form): a call whose exception
//! IS the asserted behavior.
//!
//! Entered only for the diagnostic whose CALL is the DIRECT subject of
//! `assert_raises`/`assert_raise` (the minitest block body) or of an
//! `expect { ... }.to raise_error(...)` block (`RSpec`). Measured corpus
//! site: `spec/lib/guardian/tag_guardian_spec.rb:98-100`, where the
//! spec deliberately omits the required tag inside
//! `expect { ... }.to raise_error(ArgumentError)`.
//!
//! The scope is deliberately one call wide: a call nested any deeper in
//! the block keeps firing, which is what `nested_subject_still_accuses`
//! pins. Fixtures live in `testdata/asserted_raise/` and are
//! MRI-executable — the two `*_silent` fixtures exit 0, the two
//! `*_accuses` fixtures really raise `ArgumentError` on their diagnosed
//! line (verified with ruby 3.4.2).

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/asserted_raise");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap();
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, path.clone().into(), text.clone());
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

fn only(name: &str) -> String {
    let diags = check_fixture(name);
    assert_eq!(
        diags.len(),
        1,
        "{name} must be accused exactly once, got: {diags:?}"
    );
    diags[0].clone()
}

fn silent(name: &str) {
    let diags = check_fixture(name);
    assert!(diags.is_empty(), "{name} must be silent, got: {diags:?}");
}

/// SILENT: `expect { Guardian.new.can_edit_tag? }.to raise_error(...)` —
/// the arity error is the assertion's whole point. MRI catches the
/// `ArgumentError` inside the matcher and exits 0.
#[test]
fn rspec_expect_raise_error_subject_is_silent() {
    silent("rspec_expect_raise_error_silent.rb");
}

/// SILENT: the minitest spelling of the same assertion —
/// `assert_raises(ArgumentError) { Guardian.new.can_edit_tag? }`. MRI
/// catches the `ArgumentError` and exits 0.
#[test]
fn minitest_assert_raises_subject_is_silent() {
    silent("minitest_assert_raises_silent.rb");
}

/// SILENT: minitest's older singular spelling, `assert_raise`, is the same
/// assertion. MRI catches the `ArgumentError` and exits 0.
#[test]
fn minitest_assert_raise_singular_spelling_is_silent() {
    silent("minitest_assert_raise_singular_silent.rb");
}

/// ACCUSED (line 14): the very same zero-argument call, outside any
/// assertion. MRI raises `ArgumentError` at line 14.
#[test]
fn arity_error_outside_an_assertion_still_accuses() {
    let d = only("arity_outside_assertion_accuses.rb");
    assert!(d.starts_with("14:"), "expected the bare call, got: {d}");
    assert!(d.contains("E0102"), "expected E0102, got: {d}");
    assert!(
        d.contains("can_edit_tag?") && d.contains("expects 1 argument, got 0"),
        "expected the arity message, got: {d}"
    );
}

/// ACCUSED (line 34): `expect { ... }.to eq(...)` asserts nothing about
/// exceptions, so the arm does not apply. MRI: the block really raises
/// `ArgumentError`, which this matcher does not catch.
#[test]
fn expect_with_a_non_raise_matcher_still_accuses() {
    let d = only("expect_with_another_matcher_accuses.rb");
    assert!(d.starts_with("34:"), "expected the subject call, got: {d}");
    assert!(d.contains("E0102"), "expected E0102, got: {d}");
    assert!(
        d.contains("can_edit_tag?") && d.contains("expects 1 argument, got 0"),
        "expected the arity message, got: {d}"
    );
}

/// ACCUSED (line 45): the arm is one call wide. The direct subject here IS
/// a call (`record(...)`), but the diagnosed call sits inside it — a
/// containing span must not silence it. MRI proves the inner call really
/// raises `ArgumentError` inside the asserted block.
#[test]
fn call_nested_inside_the_armed_subject_span_still_accuses() {
    let d = only("nested_call_inside_armed_subject_accuses.rb");
    assert!(d.starts_with("45:"), "expected the nested call, got: {d}");
    assert!(d.contains("E0102"), "expected E0102, got: {d}");
}

/// ACCUSED (line 39): the subject sits one level deeper — inside an array
/// literal within the `expect` block — so the arity error keeps firing.
/// That is the narrow scope the owner chose, and this is its control; MRI
/// proves the call really does raise `ArgumentError` inside the block.
#[test]
fn call_nested_deeper_in_the_asserted_block_still_accuses() {
    let d = only("nested_subject_still_accuses.rb");
    assert!(d.starts_with("39:"), "expected the nested call, got: {d}");
    assert!(d.contains("E0102"), "expected E0102, got: {d}");
    assert!(
        d.contains("can_edit_tag?") && d.contains("expects 1 argument, got 0"),
        "expected the arity message, got: {d}"
    );
}