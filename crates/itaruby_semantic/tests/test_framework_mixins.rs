//! Bead ita-se5: Mocha (`stubs`/`expects`/`unstub`) and Minitest
//! (`stub`) mix instance methods into `Object` when the test framework
//! gem is required — a project class calling `.stubs(:x)`/`.stub(:x, v)
//! { ... }` on an object it never defines those names on used to look
//! like a genuine `NotFound` under closed-world, producing a false
//! E0101 (Round-5 measurement: 11 discourse sites on Mocha, 4 rails
//! sites on Minitest's `stub` — all 15 already inside `spec/`/`test/`
//! files). Fix: `core::TEST_FRAMEWORK_OBJECT_MIXIN_METHODS`, consulted
//! through the EXISTING `core::kernel_object_instance_method` call in
//! `index.rs::soften_not_found` — no new wiring, same fallback gap as
//! Kernel's own private surface (`proc`, `lambda`, ...).
//!
//! Fixtures live under `testdata/test_framework_mixins/`, each with
//! globally unique class names (`TestFwMixin*`) — `testdata/` is
//! scanned as a single merged project by `ita check testdata/`.

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/test_framework_mixins");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap();
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, path.clone().into(), text);
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    itaruby_semantic::ClosedWorld::new(&db, true);
    itaruby_semantic::check_file(&db, file)
        .iter()
        .map(|d| format!("{}[{}]: {}", if d.severity == itaruby_semantic::Severity::Error { "error" } else { "warning" }, d.code, d.message))
        .collect()
}

/// SILENT: `obj.stubs(:real_method)` / `.expects(:real_method)` /
/// `.unstub(:real_method)` — none of `stubs`/`expects`/`unstub` exist
/// on `TestFwMixinMochaTarget`'s own ancestry, but Mocha mixes them
/// into every `Object` at test-framework require time.
#[test]
fn mocha_stubs_expects_unstub_stay_silent() {
    let diags = check_fixture("mocha_stubs_expects_silent.rb");
    assert!(diags.is_empty(), "expected silence, got: {diags:?}");
}

/// SILENT: `target.stub(:real_method, 99) { ... }` — `stub` itself is
/// the `Minitest::Mock` monkeypatch on `Object`, not a method
/// `TestFwMixinMinitestTarget` defines.
#[test]
fn minitest_stub_call_stays_silent() {
    let diags = check_fixture("minitest_stub_silent.rb");
    assert!(diags.is_empty(), "expected silence, got: {diags:?}");
}

/// CONTROLE: a name that is neither a real method, a Kernel/Object
/// method, nor one of the four test-framework mixin names must keep
/// accusing — proves the fix is a narrow allowlist of exact names, not
/// a blanket softening of every call on a class shaped like a spec.
#[test]
fn unrelated_unknown_method_still_accuses() {
    let diags = check_fixture("unrelated_unknown_method_still_accuses.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0101"), "expected E0101, got: {diags:?}");
    assert!(diags[0].contains("totally_bogus_method"), "got: {diags:?}");
}
