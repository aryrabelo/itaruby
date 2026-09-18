//! Bead ita-hfn: `#: ?{ (?) -> untyped } -> String` — an optional (or
//! required) block whose OWN parameter list is `(?)`, the RBS spec's
//! "untyped function" placeholder (github.com/ruby/rbs docs/syntax.md's
//! `_block?_` production). Measured FP: tapioca's own
//! `lib/tapioca/helpers/test/isolation.rb:30,75`, accepted verbatim by
//! `srb tc`, previously rejected here with "expected identifier at offset
//! 5" (E0105) — the last residual of ita-p24's block-type coverage.
//! `rbs_comment.rs` owns the parser and its pure-function unit tests; this
//! file exercises the same forms end-to-end through `check_file`, the real
//! caller (`index.rs`'s `sig_comments` lookup, only for a `#:` comment
//! directly above a `def`).
//!
//! MUTANTS THIS FILE MUST CATCH:
//!   1. `block_param_list`'s `(?)` special-case removed/reverted —
//!      `tapioca_shape_is_silent_and_return_type_still_checked` re-emits
//!      the historical E0105 ("expected identifier at offset 5").
//!   2. The `(?)` special-case moved out of `block_param_list` into the
//!      shared `param_list` (i.e. accepted ANYWHERE a param list starts,
//!      not just a block's) — `qmark_outside_block_position_still_warns`
//!      flips from E0105 to silent.

fn diags_of(name: &str, text: &str) -> Vec<String> {
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, format!("/p/{name}").into(), text.to_string());
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    itaruby_semantic::check_file(&db, file)
        .iter()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect()
}

fn check_fixture(name: &str) -> Vec<String> {
    let base = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/qmark_block_params");
    let path = format!("{base}/{name}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    diags_of(name, &text)
}

/// The tapioca shape itself: zero E0105, AND the method's own `-> String`
/// return type is still enforced (proven by a real E0103 downstream, not
/// the whole sig quietly degrading to Unknown along with the block).
#[test]
fn tapioca_shape_is_silent_and_return_type_still_checked() {
    let diags = check_fixture("tapioca_shape_silent.rb");
    let e0105: Vec<_> = diags.iter().filter(|d| d.starts_with("E0105")).collect();
    assert!(e0105.is_empty(), "expected zero E0105, got: {e0105:?} (all diags: {diags:?})");
    assert!(
        diags.iter().any(|d| d.starts_with("E0103")),
        "isolate_from_fork's own `-> String` return type must still be checked \
         (String fed into an Integer-only sink must fire E0103), got: {diags:?}"
    );
}

/// Regression: a block with real (non-`(?)`) params keeps parsing exactly
/// as before.
#[test]
fn real_params_block_still_silent() {
    let diags = check_fixture("real_params_block_still_parses.rb");
    let e0105: Vec<_> = diags.iter().filter(|d| d.starts_with("E0105")).collect();
    assert!(e0105.is_empty(), "expected zero E0105, got: {e0105:?} (all diags: {diags:?})");
}

/// Control ("prova dos dois lados"): `(?)` as the METHOD's OWN top-level
/// param list, not inside a block clause, must still raise E0105 — proves
/// the fix is scoped to `block_param_list` and never leaked into the
/// shared `param_list` a top-level sig also calls.
#[test]
fn qmark_outside_block_position_still_warns() {
    let diags = check_fixture("qmark_outside_block_still_warns.rb");
    let e0105: Vec<_> = diags.iter().filter(|d| d.starts_with("E0105")).collect();
    assert_eq!(e0105.len(), 1, "expected exactly 1 E0105, got: {diags:?}");
}

/// Pure end-to-end regression lock, independent of the fixture files: the
/// tapioca shape inline, on a bare-block (no leading `(...)`) def.
#[test]
fn inline_tapioca_shape_is_silent() {
    let diags = diags_of(
        "qmark_block_inline.rb",
        "class QmarkBlockInline\n  #: ?{ (?) -> untyped } -> String\n  def go\n    \"x\"\n  end\nend\n",
    );
    assert!(diags.iter().all(|d| !d.starts_with("E0105")), "unexpected E0105: {diags:?}");
}

/// Inline counter-proof: `(?)` mixed with a real param inside the SAME
/// block clause (`{ (Integer, ?) -> untyped }`) must still warn — the
/// placeholder is only ever the whole list, never one of several params.
#[test]
fn inline_qmark_mixed_with_real_param_still_warns() {
    let diags = diags_of(
        "qmark_block_mixed.rb",
        "class QmarkBlockMixedInline\n  #: ?{ (Integer, ?) -> untyped } -> String\n  def go\n    \"x\"\n  end\nend\n",
    );
    assert!(diags.iter().any(|d| d.starts_with("E0105")), "expected E0105, got: {diags:?}");
}
