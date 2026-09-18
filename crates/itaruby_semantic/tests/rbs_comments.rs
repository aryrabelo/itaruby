//! Bead ita-p24: `#:` signature comments using the documented
//! sorbet.org/docs/rbs-support forms (bare arrow, positional/rest parameter
//! names, postfix-nilable types, block/proc types, generic type params,
//! tuple types, singleton types) must not raise E0105 — measured against
//! ruby-lsp/tapioca (Shopify, `typed: strict`), where 1019/781 uses of these
//! exact forms were previously rejected. `rbs_comment.rs` owns the parser
//! and its pure-function unit tests; this file exercises the same forms
//! end-to-end through `check_file`, the real caller (`index.rs`'s
//! `sig_comments` lookup, only for a `#:` comment directly above a `def`).
//!
//! MUTANTS THIS FILE MUST CATCH:
//!   1. `parse_rbs_comment` reverted to requiring a leading `(` (the bare
//!      arrow / bare block regression) — `documented_forms_never_warn_e0105`
//!      goes from 0 E0105s to several.
//!   2. Any of `skip_block_type`/`skip_type_params`/proc-type parsing made
//!      permissive enough to swallow truncated input (e.g. an early `return
//!      Ok(())` before checking for the closing delimiter) —
//!      `malformed_forms_still_warn_e0105` drops below 6 E0105s.

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
    let base = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/rbs_comment_forms");
    let path = format!("{base}/{name}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    diags_of(name, &text)
}

/// Every documented form (bare arrow, named positional/rest params, postfix
/// nilable, required/optional block, bare block, generic method type param,
/// proc type, parenthesized union return) parses cleanly through the real
/// `def`-adjacent `#:` comment path — zero E0105s.
#[test]
fn documented_forms_never_warn_e0105() {
    let diags = check_fixture("documented_forms_silent.rb");
    let e0105: Vec<_> = diags.iter().filter(|d| d.starts_with("E0105")).collect();
    assert!(e0105.is_empty(), "expected zero E0105, got: {e0105:?} (all diags: {diags:?})");
}

/// The other side: truncated/malformed variants of the same new shapes
/// still raise E0105, one per method — the parser being generous about
/// documented syntax must not make it generous about garbage.
#[test]
fn malformed_forms_still_warn_e0105() {
    let diags = check_fixture("malformed_still_warns.rb");
    let e0105: Vec<_> = diags.iter().filter(|d| d.starts_with("E0105")).collect();
    assert_eq!(e0105.len(), 6, "expected 6 E0105s (one per malformed def), got: {diags:?}");
}

/// Pure end-to-end regression lock, independent of the fixture files: an
/// inline bare-arrow zero-param sig (the single most common corpus shape,
/// 250/1063 in ruby-lsp) must not warn.
#[test]
fn inline_bare_arrow_is_silent() {
    let diags = diags_of(
        "bare_arrow.rb",
        "class BareArrowInline\n  #: -> void\n  def go\n  end\nend\n",
    );
    assert!(diags.iter().all(|d| !d.starts_with("E0105")), "unexpected E0105: {diags:?}");
}

/// Inline counter-proof: a bare arrow with trailing junk must still warn.
#[test]
fn inline_bare_arrow_with_junk_still_warns() {
    let diags = diags_of(
        "bare_arrow_junk.rb",
        "class BareArrowJunkInline\n  #: -> void extra\n  def go\n  end\nend\n",
    );
    assert!(diags.iter().any(|d| d.starts_with("E0105")), "expected E0105, got: {diags:?}");
}
