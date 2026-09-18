//! Bead ita-yh1: a `::`-prefixed constant path (cbase) resolves from the
//! TOP LEVEL ONLY in real Ruby — never through lexical nesting. The
//! checker used to trim the leading `::` off the path before falling back
//! to the diagnostic-path lookup (`check.rs::infer_const` passed a
//! `trimmed` path to `check_const_ref`, and `index.rs::const_exists`
//! separately trimmed the owner prefix before resolving it), so a
//! same-named lexically-nested module could SHADOW the real top-level
//! target: `::Tapioca::TAPIOCA_DIR` referenced from inside
//! `RubyLsp::Tapioca::ServerAddon` falsely resolved `Tapioca` to the
//! sibling `RubyLsp::Tapioca` (which never defines `TAPIOCA_DIR`) instead
//! of the real top-level `::Tapioca`, producing a false E0104.
//!
//! Fix: `infer_const` now passes the UNTRIMMED path through to
//! `check_const_ref` (matching what `resolve_const_through_aliases`
//! already did on the typing side), and `index.rs::const_exists` no
//! longer strips the leading `::` off the qualified-name owner prefix
//! before resolving it — both now let `resolve_const`'s own
//! `strip_prefix("::")` decide top-level-only vs lexical, instead of
//! deciding it themselves by discarding the marker first.
//!
//! MUTANTS THIS FILE MUST CATCH:
//!
//!   1. cbase detection removed: revert `check.rs::infer_const` to pass
//!      `trimmed` (the `path.trim_start_matches("::")` value) to
//!      `check_const_ref` instead of `&path` — `cbase_ignores_lexical_
//!      shadow` would wrongly diagnose E0104 again (the lexical shadow
//!      swallows the reference).
//!   2. cbase treated as "always top-level hit" without existence check:
//!      in `index.rs::const_exists`, replace the qualified-name branch's
//!      `let Some(owner) = owner else { ... }` short-circuit with an
//!      unconditional `return true` whenever `prefix` starts with `"::"`
//!      (i.e. skip `const_in_ancestors` entirely for any cbase prefix) —
//!      `cbase_missing_member_still_warns` would wrongly go silent even
//!      though `::CbaseShTarget2::DOES_NOT_EXIST` is not a real member.
//!   3. Same site, narrower form: revert the `let prefix =
//!      prefix.trim_start_matches("::");` line in `const_exists` (the
//!      exact one-line change this bead reverses) — same failure as
//!      mutant 1's fixture, `cbase_ignores_lexical_shadow` regains its
//!      false E0104.

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/cbase_shadowing");
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
fn cbase_path_resolves_to_top_level_not_lexical_shadow() {
    let diags = check_fixture("cbase_ignores_lexical_shadow.rb");
    assert!(
        diags.is_empty(),
        "`::CbaseShTarget::VALUE` must resolve from the TOP LEVEL only \
         (cbase semantics), never through the lexically-nested \
         `CbaseShHost::CbaseShTarget` shadow of the same simple name — \
         got: {diags:?}"
    );
}

#[test]
fn cbase_path_to_real_module_missing_member_still_warns() {
    let diags = check_fixture("cbase_missing_member_still_warns.rb");
    assert_eq!(
        diags.len(),
        1,
        "expected exactly 1 diagnostic (E0104 on the genuinely undefined \
         `::CbaseShTarget2::DOES_NOT_EXIST`) — a cbase fix that resolves \
         the head to the top level must not also skip the member \
         existence check, got: {diags:?}"
    );
    assert!(diags[0].contains("E0104"), "expected E0104, got: {:?}", diags[0]);
    assert!(
        diags[0].contains("DOES_NOT_EXIST"),
        "message should name the unresolved constant, got: {:?}",
        diags[0]
    );
}

#[test]
fn relative_path_still_resolves_through_lexical_shadow_and_warns() {
    // Pins CURRENT (and Ruby-correct) behavior for the non-cbase sibling
    // of the repro: without a leading `::`, real Ruby lookup starts from
    // lexical nesting, so `CbaseShTarget3` legitimately resolves to the
    // nested `CbaseShHost3::CbaseShTarget3` shadow (not the unrelated
    // top-level module of the same name), and that shadow has no `VALUE`
    // member — a real NameError in Ruby, so itaruby's E0104 here is
    // correct and must NOT be affected by the cbase fix.
    let diags = check_fixture("relative_path_resolves_lexical_shadow.rb");
    assert_eq!(
        diags.len(),
        1,
        "expected exactly 1 diagnostic (E0104: relative `CbaseShTarget3` \
         correctly resolves to the lexical shadow, which lacks `VALUE`) — \
         got: {diags:?}"
    );
    assert!(diags[0].contains("E0104"), "expected E0104, got: {:?}", diags[0]);
}
