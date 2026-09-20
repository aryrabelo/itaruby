//! The dark singleton census: what the class-object track WOULD accuse.
//!
//! The `Ty::Class` `MethodLookup::NotFound`/`Inconclusive` arms are silent on
//! purpose (the known extend/concern gaps, `singleton_lookup.rs`). This suite
//! pins the census that measures that silence two-sidedly:
//!
//! * the two gap fixtures must bucket `closed_notfound` — the residue, i.e.
//!   exactly what a class-object E0101 fires on if the track ever arms;
//! * a receiver stood down by a dynamic-definer mark must bucket `open` —
//!   the test a bucketing mutant fails (one that ignores the blocker and
//!   labels every `NotFound` closed would flip `open_receiver_typo.rb`);
//! * resolved calls and non-project receivers record nothing at all.
//!
//! The census reads nothing the walk does not already read, so every
//! assertion here doubles as the zero-diagnostic-change proof for these
//! fixtures: `diags` is asserted empty alongside the buckets.

use itaruby_semantic::{check_file_dark, Db, LineIndex, ProjectFiles, SourceFile};

/// (rendered diagnostics, records as `receiver method verdict-tag`).
fn dark_fixture(name: &str) -> (Vec<String>, Vec<String>) {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/dark_singleton");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap();
    let db = Db::default();
    let file = SourceFile::new(&db, path.clone().into(), text.clone());
    ProjectFiles::new(&db, vec![file]);
    let li = LineIndex::new(&text);
    let (diags, recs) = check_file_dark(&db, file);
    let rendered = diags
        .iter()
        .map(|d| {
            let (l, c) = li.line_col(&text, d.start);
            format!("{}:{}:{} {}", l + 1, c + 1, d.code, d.message)
        })
        .collect();
    let tags = recs
        .into_iter()
        .map(|r| {
            let v = match r.verdict {
                itaruby_semantic::DarkVerdict::ClosedNotFound => "closed_notfound".to_string(),
                itaruby_semantic::DarkVerdict::Open(reason) => format!("open({reason})"),
            };
            format!("{} {} {}", r.receiver, r.method, v)
        })
        .collect();
    (rendered, tags)
}

/// GAP 1: the extend-provided class-method typo is the residue — bucketed
/// `closed_notfound`, still no diagnostic. (`Page.extend` itself also
/// buckets `open(inconclusive_no_blocker)`: the missing Class/Module tail —
/// measured here first.)
#[test]
fn extend_typo_is_closed_notfound_and_still_silent() {
    let (diags, recs) = dark_fixture("extend_typo.rb");
    assert!(diags.is_empty(), "gap changed: {diags:?}");
    assert!(
        recs.iter().any(|r| r == "Page find_by_slog closed_notfound"),
        "the extend typo must be the residue bucket: {recs:?}"
    );
    assert_eq!(
        recs.iter().filter(|r| r.ends_with("closed_notfound")).count(),
        1,
        "exactly one residue site — nothing else in the fixture reads closed: {recs:?}"
    );
}

/// GAP 2: the concern-backbone typo, same expectation. (`Record.include`
/// buckets `open(inconclusive_no_blocker)` like gap 1's `extend`.)
#[test]
fn included_hook_typo_is_closed_notfound_and_still_silent() {
    let (diags, recs) = dark_fixture("included_hook_typo.rb");
    assert!(diags.is_empty(), "gap changed: {diags:?}");
    assert!(
        recs.iter()
            .any(|r| r == "Record default_scope_nmae closed_notfound"),
        "the concern typo must be the residue bucket: {recs:?}"
    );
    assert_eq!(
        recs.iter().filter(|r| r.ends_with("closed_notfound")).count(),
        1,
        "exactly one residue site: {recs:?}"
    );
}

/// THE bucketing guard: an unrecognized class-body call stands the receiver
/// down, so the typo buckets `open`, never `closed_notfound`. A mutant that
/// ignores the blocker labels every NotFound closed and fails here.
#[test]
fn unrecognized_class_body_call_keeps_the_receiver_open() {
    let (diags, recs) = dark_fixture("open_receiver_typo.rb");
    assert!(diags.is_empty(), "an open receiver never diagnoses: {diags:?}");
    // Both class-object sites bucket open — the DSL call AND the typo it
    // stands down; not one of them may read closed.
    assert!(
        recs.contains(&"Widget load_nmae open(Project(UnknownClassBodyCall))".to_string()),
        "the stood-down typo must bucket open with the body-call blocker: {recs:?}"
    );
    assert!(
        recs.iter().all(|r| !r.ends_with("closed_notfound")),
        "an open receiver never reaches the residue bucket: {recs:?}"
    );
}

/// Resolved calls never enter the residue bucket. (`Page.extend` itself
/// buckets `open(inconclusive_no_blocker)` — the missing tail, as in the
/// gap fixtures; the RESOLVED `find_by_slug` records nothing.)
#[test]
fn resolved_call_records_nothing() {
    let (diags, recs) = dark_fixture("extend_correct.rb");
    assert!(diags.is_empty(), "correct code stays silent: {diags:?}");
    assert!(
        recs.iter().all(|r| !r.ends_with("closed_notfound")),
        "a resolved lookup is never residue: {recs:?}"
    );
}

/// Non-project receivers (the SecureRandom/Kernel FP population) never reach
/// the class-object arms and so never bucket.
#[test]
fn foreign_receiver_records_nothing() {
    let (diags, recs) = dark_fixture("stdlib_receiver.rb");
    assert!(diags.is_empty(), "{diags:?}");
    assert!(recs.is_empty(), "no class-object site, no record: {recs:?}");
}
