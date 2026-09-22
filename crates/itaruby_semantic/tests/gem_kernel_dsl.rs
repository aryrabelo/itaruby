//! Bead ita-yxc: bare (implicit-receiver) calls into Kernel/top-level DSL
//! methods a PUBLIC gem mixes into `Object`/`Kernel` at runtime —
//! `Stoplight(...)` (stoplight gem), `Rainbow(...)` (rainbow gem), `_`/`s_`
//! (`fast_gettext`'s `FastGettext::Translation`, included in `Object`).
//! Round-5 audit across the four public corpora (2026-08-25) measured
//! these misresolving to a false `E0101` on every project class, since no
//! project ever defines them and no core inventory can enumerate a gem it
//! never loaded. Fixtures live under `testdata/gem_kernel_dsl/`:
//! `silence.rb` proves the fix (no diagnostics), `control.rb` proves the
//! allowlist is narrow (a genuinely unknown bare name still warns).
//! Mechanism: `core::GEM_KERNEL_METHODS`, consulted by
//! `core::kernel_object_instance_method` — same fallback path bead ita-4xy
//! already wired for Ruby's own Kernel private methods
//! (`crates/itaruby_semantic/tests/lookup_gaps.rs`), no `index.rs`/
//! `check.rs` change needed.

fn sev(s: itaruby_semantic::Severity) -> &'static str {
    match s {
        itaruby_semantic::Severity::Error => "error",
        itaruby_semantic::Severity::Warning => "warning",
    }
}

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/gem_kernel_dsl");
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

/// G1: `Stoplight(...)`/`Rainbow(...)`/`_(...)`/`s_(...)` bare calls, each
/// on its own closed project class, must produce zero diagnostics.
#[test]
fn gem_kernel_dsl_bare_calls_stay_silent() {
    let diags = check_fixture("silence.rb");
    assert!(
        diags.is_empty(),
        "Stoplight/Rainbow/_/s_ are curated public-gem Kernel-level DSL \
         methods (bead ita-yxc): a bare self-send must never accuse E0101, \
         got: {diags:?}"
    );
}

/// G2: negative control — a bare call to a name genuinely absent from the
/// allowlist, on an otherwise identical closed project class, must still
/// accuse E0101. Proves `GEM_KERNEL_METHODS` narrowly allowlists the four
/// measured names rather than softening every bare miss.
#[test]
fn unrelated_bare_call_still_warns_e0101() {
    let diags = check_fixture("control.rb");
    assert_eq!(diags.len(), 1, "expected exactly one E0101, got: {diags:?}");
    assert!(
        diags[0].contains("E0101"),
        "expected E0101 on the genuinely unknown bare call, got: {diags:?}"
    );
    assert!(
        diags[0].contains("gem_kernel_dsl_totally_unknown_bare_call"),
        "expected the diagnostic to name the unresolved method, got: {diags:?}"
    );
}

/// Bead ita-bgd: `BigDecimal(...)` is the same shape one gem further out
/// — a bundled gem's Kernel conversion function, invisible to
/// `gen-core-inventory.rb`'s `--disable-gems` harvest for exactly the
/// reason `gem` and `URI` are. Measured as 4 of rails' 33 class-object
/// census residue records, all in a module BODY (implicit receiver), so
/// the receiver is the module's class object.
#[test]
fn bundled_gem_kernel_conversion_function_stays_silent() {
    let diags = check_fixture("bigdecimal.rb");
    assert!(
        diags.is_empty(),
        "`Kernel#BigDecimal` comes from the bundled bigdecimal gem: a bare \
         call must never accuse E0101, got: {diags:?}"
    );
}
