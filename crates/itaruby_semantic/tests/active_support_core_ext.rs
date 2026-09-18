//! Beads ita-o8l.2 / ita-o8l.3: `ActiveSupport`'s `Object` `core_ext`
//! (`to_json`, `try`, `instance_values`, `acts_like?`) and `RubyGems`'
//! `Kernel#gem`, absent from ita's generated core inventory because
//! neither `ActiveSupport` nor `RubyGems` is loaded when
//! `gen-core-inventory.rb` harvests (it deliberately runs `ruby
//! --disable-gems`, and activesupport's own `.rbs` reopens `Object`,
//! which `core_stdlib_tops` filters out of the pack on purpose --
//! wave 9 defect 1). Round-5/precision-report measurement (2026-08-25):
//! 14 confirmed `to_json`/`try`/`instance_values`/`acts_like?` sites (13
//! rails + 1 discourse) plus 1 confirmed `gem` site (rails). Fixtures
//! live under `testdata/o8l_core_ext/`: `silence.rb` proves the fix (no
//! diagnostics), `control.rb` proves the allowlist is narrow (a
//! genuinely unknown bare/receiver call still warns). Mechanism:
//! `core::ACTIVE_SUPPORT_OBJECT_MIXIN_METHODS` (the four `core_ext` names)
//! and an added `"gem"` entry in `core::KERNEL_PRIVATE_INSTANCE_METHODS`,
//! both consulted by `core::kernel_object_instance_method` -- same
//! fallback path bead ita-yxc already wired for public-gem Kernel DSL
//! (`crates/itaruby_semantic/tests/gem_kernel_dsl.rs`), no
//! `index.rs`/`check.rs` change needed.

fn sev(s: itaruby_semantic::Severity) -> &'static str {
    match s {
        itaruby_semantic::Severity::Error => "error",
        itaruby_semantic::Severity::Warning => "warning",
    }
}

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/o8l_core_ext");
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

/// `to_json`, `try`, `instance_values`, `gem` (all bare self-sends) and
/// `acts_like?` (explicit receiver falling back to `Object`) must all
/// produce zero diagnostics.
#[test]
fn active_support_core_ext_and_gem_stay_silent() {
    let diags = check_fixture("silence.rb");
    assert!(
        diags.is_empty(),
        "to_json/try/instance_values/acts_like?/gem are curated Object/\
         Kernel mixin methods (beads ita-o8l.2, ita-o8l.3): none may ever \
         accuse E0101, got: {diags:?}"
    );
}

/// Negative control -- an invented bare-call name and an invented
/// explicit-receiver name, on the same closed-project-class shapes as
/// `silence.rb`, must both still accuse E0101. Proves the allowlist is
/// narrow (five measured names), not a blanket Kernel/Object
/// suppressor.
#[test]
fn unrelated_calls_still_warn_e0101() {
    let diags = check_fixture("control.rb");
    assert_eq!(diags.len(), 2, "expected exactly two E0101s, got: {diags:?}");
    for d in &diags {
        assert!(d.contains("E0101"), "expected E0101, got: {d}");
    }
    assert!(
        diags.iter().any(|d| d.contains("o8l_core_ext_totally_unknown_bare_call")),
        "expected the bare-call E0101 to name the unresolved method, got: {diags:?}"
    );
    assert!(
        diags.iter().any(|d| d.contains("o8l_core_ext_totally_unknown_receiver_call")),
        "expected the receiver-call E0101 to name the unresolved method, got: {diags:?}"
    );
}
