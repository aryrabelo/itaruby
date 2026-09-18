//! Bead ita-d2: `core_inventory.txt` never harvested constants — only
//! `Class#method` lines for a fixed list of non-exception classes — so
//! every core exception class the generator's `CLASSES` list skipped
//! (proven: `SystemStackError`) fell through to `is_known_core_constant`'s
//! hand list, which also lacked it, and warned a false E0104. Fixed by
//! teaching `scripts/gen-core-inventory.rb` to also harvest
//! `Object.constants` (one `::Name` line per top-level constant) and
//! wiring `core.rs::is_known_core_constant` to consult the new
//! `core_inventory_has_const` lookup additively (a hit there can only ADD
//! a suppression, never remove one the hand list already granted).
//!
//! Fixtures live in `testdata/core_consts/`:
//! `rescue_system_stack_error_silent.rb` proves the fix (no E0104 on
//! `rescue SystemStackError`); `rescue_nonexistent_control_still_warns.rb`
//! is the control, same shape, a genuinely nonexistent name — must still
//! accuse E0104, proving the fix is additive rather than a blanket
//! softening of the rescue-clause constant check.
//!
//! Mutant table (bead ita-d2 acceptance):
//!   (a) delete the `::SystemStackError` line from the regenerated
//!       `core_inventory.txt` (simulate the pre-fix hole) ->
//!       `rescue_system_stack_error_silent.rb` re-emits E0104.
//!   (b) generator run WITHOUT `--disable-gems` (guard bypass) ->
//!       `scripts/gen-core-inventory.rb` itself aborts before emitting a
//!       single line (`generator_aborts_without_disable_gems_guard`
//!       below drives the real script as a subprocess and asserts the
//!       abort).

fn sev(s: itaruby_semantic::Severity) -> &'static str {
    match s {
        itaruby_semantic::Severity::Error => "error",
        itaruby_semantic::Severity::Warning => "warning",
    }
}

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/core_consts");
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
            format!("{}:{}:{} {} {}", l + 1, c + 1, sev(d.severity), d.code, d.message)
        })
        .collect()
}

#[test]
fn rescue_system_stack_error_resolves_silently() {
    let diags = check_fixture("rescue_system_stack_error_silent.rb");
    assert!(
        diags.is_empty(),
        "SystemStackError is a real core exception class harvested from \
         `Object.constants`: no E0104 (or anything else), got: {diags:?}"
    );
}

#[test]
fn rescue_nonexistent_control_still_warns_e0104() {
    let diags = check_fixture("rescue_nonexistent_control_still_warns.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0104"), "expected E0104, got: {:?}", diags[0]);
    assert!(
        diags[0].contains("CoreConstsTotallyMadeUpError"),
        "message should name the unresolved constant, got: {:?}",
        diags[0]
    );
}

/// Direct unit-level check of the two new/changed lookups (bead ita-d2),
/// mirroring `core.rs`'s own `known_core_constants` test but naming the
/// exact constant the fixture above exercises.
#[test]
fn core_inventory_has_const_and_is_known_core_constant_agree() {
    use itaruby_semantic::core::{core_inventory_has_const, is_known_core_constant};
    assert!(core_inventory_has_const("SystemStackError"));
    assert!(is_known_core_constant("SystemStackError"));
    assert!(!core_inventory_has_const("CoreConstsTotallyMadeUpError"));
    assert!(!is_known_core_constant("CoreConstsTotallyMadeUpError"));
    // Additive contract: every hand-list name must still hit (mutant (a)'s
    // sibling — the OR must never have flipped into an AND / replacement).
    assert!(is_known_core_constant("Comparable"));
    assert!(is_known_core_constant("ENV"));
}

/// Mutant (b) of the acceptance table: the generator's own second guard
/// must abort the moment `--disable-gems` is dropped, before it ever
/// emits a line that could be a gem's, not the language's. Runs the real
/// script as a subprocess under plain `ruby` (no flag) so this is a
/// genuine end-to-end proof, not a description of the guard.
#[test]
fn generator_aborts_without_disable_gems_guard() {
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scripts/gen-core-inventory.rb");
    let output = std::process::Command::new("ruby")
        .arg(script)
        .output()
        .expect("failed to spawn ruby — is it on PATH?");
    assert!(
        !output.status.success(),
        "generator must abort when run without --disable-gems, got exit {:?} stdout={:?}",
        output.status,
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).trim().is_empty(),
        "generator must emit nothing on stdout before aborting, got: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    // The generator refuses TWO unsafe environments, and which refusal
    // fires first depends on the host: on Ruby >= 3.0 without the flag it
    // names --disable-gems; on an old host ruby (e.g. macOS system 2.6)
    // the version guard fires first. Both are correct refusals — the
    // contract this test defends is "aborts with nothing on stdout",
    // never which guard got there first (learned 2026-08-26: pinning the
    // message made the test's verdict depend on the host ruby version).
    assert!(
        stderr.contains("--disable-gems") || stderr.contains("need Ruby >="),
        "abort message should name a refused environment, got: {stderr:?}"
    );
}
