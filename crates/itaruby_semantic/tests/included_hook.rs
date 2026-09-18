//! Bead ita-1yw: the canonical `def self.included(base)` hook calling
//! `base.attr_accessor :x` / `base.define_method(:x)` defines instance
//! methods on every includer, but def bodies are never walked — the
//! includer's calls were false E0101s (7 sites measured in a public
//! discourse clone during the round-5 audit). The harvester models ONLY the
//! literal shape; everything else is a silent no-op (fail-closed false
//! negative, invariant #1).
//!
//! Fixtures live under `testdata/included_hook/` with an `IncHook` class-name
//! prefix — `testdata/` is scanned as one merged project by
//! `ita check testdata/` (gate c), so names must stay globally unique.
//!
//! MUTANT THIS FILE MUST CATCH: commenting out the single call site
//! `self.harvest_included_hook(i, &def);` flips exactly the two silence tests
//! back to the historical E0101s (`title`/`title=`/`greet`), while both
//! controls stay green — neither control exercises the new helper at all.

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/included_hook");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, path.clone().into(), text);
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    itaruby_semantic::check_file(&db, file)
        .iter()
        .map(|d| format!("{}[{}]: {}", if d.severity == itaruby_semantic::Severity::Error { "error" } else { "warning" }, d.code, d.message))
        .collect()
}

#[test]
fn attr_accessor_via_included_hook_silences() {
    let diags = check_fixture("attr_accessor_silent.rb");
    assert!(diags.is_empty(), "hook attr_accessor must resolve on the includer, got: {diags:?}");
}

#[test]
fn define_method_via_included_hook_silences() {
    let diags = check_fixture("define_method_silent.rb");
    assert!(diags.is_empty(), "hook define_method must resolve on the includer, got: {diags:?}");
}

#[test]
fn no_hook_still_accuses() {
    let diags = check_fixture("no_hook_still_warns.rb");
    assert_eq!(diags.len(), 1, "module without hook must keep accusing: {diags:?}");
    assert!(diags[0].contains("E0101"), "expected E0101: {diags:?}");
}

#[test]
fn non_literal_hook_never_over_registers() {
    let diags = check_fixture("dynamic_hook_still_warns.rb");
    assert_eq!(diags.len(), 1, "class_eval(string) hook must NOT register `secret`: {diags:?}");
    assert!(diags[0].contains("E0101") && diags[0].contains("secret"), "expected E0101 on `secret`: {diags:?}");
}
