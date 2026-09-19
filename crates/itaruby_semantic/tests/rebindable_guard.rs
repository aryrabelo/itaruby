//! Bead ita-w2c, bead F: the rebindable-block guard sits ABOVE the
//! method-lookup dispatch, so the Found/arity/sig path consults it too.
//!
//! A receiverless call inside a block whose receiving call could rebind
//! `self` (`instance_exec`-style) resolves against a `self` that is not
//! the enclosing class at runtime, so nothing the walk derives from that
//! class is conclusive — not a missing method (the arm the guard already
//! lived in) and not the arity of a method that happens to exist there.
//! Measured corpus site: `lib/email/message_builder.rb:200-203`, where
//! `body html` inside `Mail::Part.new { ... }` really reaches the Part's
//! one-argument setter while the enclosing builder's own `body` takes
//! none.
//!
//! Fixtures live in `testdata/rebindable_guard/`: the silent one is MRI-
//! executable and exits 0 (`Part#initialize` really `instance_eval`s the
//! block, exactly like the mail gem), the control really raises
//! NoMethodError on its diagnosed line (verified with ruby 3.4.2).

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/rebindable_guard");
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

/// SILENT: `body "html body"` inside `Part.new { ... }`. The lexical
/// class's own `body` takes zero arguments, but it is not the method that
/// runs — the block's `self` is the Part under construction. MRI exits 0.
#[test]
fn self_send_inside_a_rebindable_block_is_silent() {
    silent("rebindable_block_self_send_silent.rb");
}

/// ACCUSED (line 21): only SELF-sends are softened — an explicit-receiver
/// call inside the same rebindable block still names a receiver that
/// `instance_eval` cannot change. MRI raises NoMethodError at line 21.
#[test]
fn explicit_receiver_in_a_rebindable_block_still_accuses() {
    let d = only("explicit_receiver_in_rebindable_block_accuses.rb");
    assert!(d.starts_with("21:"), "expected the explicit-receiver call, got: {d}");
    assert!(d.contains("E0101"), "expected E0101, got: {d}");
    assert!(d.contains("::Helper"), "expected Helper named, got: {d}");
}

/// ACCUSED (line 11): a self-send inside a block whose receiving call IS
/// proven lexical — `[1, 2].each`, a core iterator that merely yields —
/// stays conclusive. MRI raises NoMethodError at line 11.
#[test]
fn self_send_inside_a_lexical_block_still_accuses() {
    let d = only("lexical_block_self_send_accuses.rb");
    assert!(d.starts_with("11:"), "expected the block body call, got: {d}");
    assert!(d.contains("E0101"), "expected E0101, got: {d}");
    assert!(
        d.contains("w2c_absent_step"),
        "expected the missing method named, got: {d}"
    );
}