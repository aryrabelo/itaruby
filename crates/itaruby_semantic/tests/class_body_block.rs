//! Class-body blocks: any call WITH a receiver carrying a block at
//! class-body level (`%i[...].each { |m| delegate m, to: :@lookup }`,
//! `FIELDS.each do ... end`) runs at class-body scope and can define
//! methods (Forwardable's `delegate`, `attr_accessor`, anything) — the
//! walker already opens the class for it. What it did NOT do before
//! 2026-09-03 was record the reason safely: `first-reason-wins` let an
//! EARLIER `AbstractRaise` swallow a LATER `ClassBodyBlock` on the same
//! class, and the abstract-raise lookup softening then treated a
//! metaprogramming-heavy class as a pure abstract stub — 23 false E0101s
//! on discourse's `script/import_scripts/*` importers (every delegated
//! name looked provably missing). The fix is REASON PRECEDENCE:
//! `AbstractRaise` is the weakest open reason; any other reason replaces
//! it (`open_class`, fragment merge, and the reopen force-open sites).
//!
//! Fixtures under `testdata/class_body_block/`.

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/class_body_block");
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

/// The discourse shape, generalized: a delegate loop in a class body
/// forwards real methods into existence at load time, so a self-send of a
/// delegated name resolves at runtime. The walker already opens the class
/// for a receiver-ful class-body block — this pins that.
#[test]
fn delegate_loop_self_send_is_silent() {
    let diags = check_fixture("delegate_loop_self_send_silent.rb");
    assert!(
        diags.is_empty(),
        "the delegate loop defines `foo` at load: no E0101. Got: {diags:?}"
    );
}

/// The control: the same class WITHOUT the loop is genuinely closed and a
/// bare self-send of a name nothing defines keeps accusing — the block
/// opening must not blanket-silence its neighbours.
#[test]
fn no_loop_still_reports() {
    let diags = check_fixture("no_loop_still_accuses.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(
        diags[0].starts_with("12:5:E0101"),
        "expected E0101 at the `foo` call, got: {:?}",
        diags[0]
    );
}

/// Documents the pre-existing receiverless shape (`included do ... end`)
/// that bead ita-o1n already opens — pinned side by side with the
/// receiver-ful loop.
#[test]
fn receiverless_block_stays_silent() {
    let diags = check_fixture("receiverless_block_already_open.rb");
    assert!(
        diags.is_empty(),
        "the receiverless class-body block already opens the class. Got: {diags:?}"
    );
}

/// The discriminating fixture for the precedence fix: `raise
/// NotImplementedError` FIRST and a delegate loop AFTER on the same class.
/// With precedence the class is ClassBodyBlock-open (silence, correct —
/// the loop defines `foo` at load). With first-reason-wins it recorded
/// `AbstractRaise` alone and the abstract-raise softening treated it as a
/// pure stub, firing a false E0101 on the delegated name — measured as 23
/// false positives on discourse's importers before the fix.
#[test]
fn abstract_raise_with_delegate_loop_is_silent() {
    let diags = check_fixture("abstract_raise_with_delegate_loop_silent.rb");
    assert!(
        diags.is_empty(),
        "the delegate loop beats the raise stub for the open reason: no E0101. Got: {diags:?}"
    );
}
