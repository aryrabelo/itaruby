//! Client-project Tapioca RBI declarations (bead ita-vto). Fixtures live
//! under `testdata/tapioca_rbi/`, mirroring `tests/declarations.rs`'s
//! pattern for the curated `gems.rbi`: `uses_rbi_gem.rb` proves the
//! constant resolves once the fixture's own `sorbet/rbi` is discovered and
//! lazily loaded (E0104 -> silence); `subclass_stays_open.rb` proves the
//! same invariant #1 contract — an RBI declaration never closes ancestry
//! for method lookup; `without_sorbet_rbi_discovered_...` is the negative
//! control, proving the "before this bead" behavior for the exact same
//! source text.

fn sev(s: itaruby_semantic::Severity) -> &'static str {
    match s {
        itaruby_semantic::Severity::Error => "error",
        itaruby_semantic::Severity::Warning => "warning",
    }
}

fn fixture_dir() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/tapioca_rbi")
}

/// Loads one fixture file plus the fixture's own toy `sorbet/rbi`, exactly
/// as `crates/itaruby/src/main.rs::run_check`/`wire_rbi` wire a real
/// project.
fn check_fixture(name: &str) -> Vec<String> {
    let dir = fixture_dir();
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap();
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, path.into(), text.clone());
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);

    let rbi_dir = std::path::Path::new(dir).join("sorbet/rbi");
    let files = itaruby_semantic::rbi::discover_rbi_files(&rbi_dir);
    assert!(!files.is_empty(), "fixture must carry at least one .rbi file");
    let index = itaruby_semantic::rbi::build_rbi_index(&files);
    itaruby_semantic::RbiProject::new(&db, index.constants);

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
fn client_rbi_constant_resolves_and_stays_silent() {
    let diags = check_fixture("uses_rbi_gem.rb");
    assert!(
        diags.is_empty(),
        "TapiocaVtoFixtureGem::Widget is declared in the toy sorbet/rbi \
         fixture: the constant must resolve (no E0104), got: {diags:?}"
    );
}

#[test]
fn client_rbi_superclass_does_not_close_ancestry_for_method_lookup() {
    let diags = check_fixture("subclass_stays_open.rb");
    assert!(
        diags.is_empty(),
        "an RBI declaration must never close ancestry: `undefined_method` on \
         a subclass of an RBI-declared class must stay silent (invariant \
         #1), got: {diags:?}"
    );
}

/// Names the acceptance criterion this bead exists to flip: the exact same
/// source text, with `RbiProject` never wired (no `sorbet/rbi` discovered —
/// the state every project was in before this bead), must still warn
/// E0104. This is the "before" half of `uses_rbi_gem.rb`'s "before: E0104,
/// after: silence" contract.
#[test]
fn without_sorbet_rbi_discovered_the_same_constant_still_warns_e0104() {
    let path = format!("{}/uses_rbi_gem.rb", fixture_dir());
    let text = std::fs::read_to_string(&path).unwrap();
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, path.into(), text.clone());
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    // RbiProject deliberately never wired — the same state `wire_rbi`'s
    // absent-directory branch leaves a project in.

    let diags = itaruby_semantic::check_file(&db, file);
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert_eq!(diags[0].code, "E0104");
    assert!(
        diags[0].message.contains("TapiocaVtoFixtureGem::Widget"),
        "message should name the unresolved constant, got: {:?}",
        diags[0].message
    );
}

/// w12 closure (Tapioca closed world): the pure collection the conclusive
/// lookup's condition (d) runs on — instance method NAMES per core
/// namespace reopening, `def self.x` excluded (it lands on the class
/// object, which no core receiver dispatches through), non-core
/// namespaces excluded.
#[test]
fn core_methods_of_collects_instance_method_names_of_core_reopenings() {
    let defs = itaruby_semantic::index::parse_defs_text(
        "# typed: true\n\nclass String\n  def squish; end\n  def self.paint; end\nend\n\nmodule Kernel\n  def tactful; end\nend\n\nclass Widget\n  def w; end\nend\n",
    );
    let methods = itaruby_semantic::core_methods_of(&defs.fragments);
    assert!(
        methods.get("String").is_some_and(|s| s.contains("squish")),
        "String#squish must be collected, got: {methods:?}"
    );
    assert!(
        !methods.get("String").is_some_and(|s| s.contains("paint")),
        "`def self.paint` is a singleton method — never collected: {methods:?}"
    );
    assert!(methods.get("Kernel").is_some_and(|s| s.contains("tactful")));
    assert!(!methods.contains_key("Widget"), "non-core namespaces stay out");
}
