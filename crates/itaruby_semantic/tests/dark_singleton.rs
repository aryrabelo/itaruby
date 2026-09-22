//! The dark singleton census: what the class-object track WOULD accuse.
//!
//! Since 2026-09-21 the `Ty::Class` `MethodLookup::NotFound` arm EMITS
//! E0101. Singleton lookup already excludes open/incomplete ancestry;
//! the RBI wrapper only softens that verdict. The census stays on
//! as the instrument that would show the next population arriving. This
//! suite pins both sides:
//!
//! * the two gap fixtures must bucket `closed_notfound` and emit E0101;
//! * a receiver stood down by a dynamic-definer mark stays silent and
//!   buckets `open`; removing the singleton lookup's open-ancestor guard
//!   must produce a false E0101, not merely change a census label;
//! * resolved calls and non-project receivers record nothing at all.
//!
//! Diagnostics are checked alongside the census labels: an open receiver
//! must stay silent, while a closed receiver with the same typo must accuse.

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
                itaruby_semantic::DarkVerdict::KnownTail => "known_tail".to_string(),
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
fn extend_typo_accuses_on_the_class_object_track() {
    let (diags, recs) = dark_fixture("extend_typo.rb");
    assert_eq!(
        diags,
        vec!["19:6:E0101 undefined method `find_by_slog` for class `Page`".to_string()],
        "the residue bucket is exactly what the armed track accuses"
    );
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
fn included_hook_typo_accuses_on_the_class_object_track() {
    let (diags, recs) = dark_fixture("included_hook_typo.rb");
    assert_eq!(
        diags,
        vec![
            "19:8:E0101 undefined method `default_scope_nmae` for class `Record`".to_string()
        ],
        "the residue bucket is exactly what the armed track accuses"
    );
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

/// CO-R: the unknown class-body call makes singleton lookup Inconclusive
/// before emission. Removing that lookup guard must fail the diagnostic
/// assertion, not just the census labels. The same typo without the DSL
/// must still accuse, so blanket singleton silence cannot pass.
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
    let (closed_diags, closed_recs) = dark_fixture("closed_receiver_typo.rb");
    assert_eq!(
        closed_diags,
        vec!["9:8:E0101 undefined method `load_nmae` for class `Widget`".to_string()],
        "without the unknown DSL, the same typo must accuse"
    );
    assert_eq!(closed_recs, vec!["Widget load_nmae closed_notfound"]);
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

/// Bead ita-tail: the headline tail call — a bare `raise` in a `def self.`
/// body — is INVENTORY silence, bucketed `known_tail`, never residue. The
/// census label is the two-sided guard: a mutant that drops `raise` (or any
/// generated private) from the tail set flips this fixture to
/// `closed_notfound` and fails here.
#[test]
fn kernel_tail_raise_buckets_known_tail_and_still_silent() {
    let (diags, recs) = dark_fixture("kernel_tail_raise_silently.rb");
    assert!(diags.is_empty(), "tail silence never diagnoses: {diags:?}");
    assert!(
        recs.iter().any(|r| r == "Importer raise known_tail"),
        "the bare raise must bucket known_tail: {recs:?}"
    );
    assert!(
        recs.iter().all(|r| !r.ends_with("closed_notfound")),
        "a tail name never reaches the residue bucket: {recs:?}"
    );
}

/// The rest of the measured tail family, same expectation, one assertion
/// per name: `rand`/`caller`/`Array`/`URI`/`puts`/`sleep`/`block_given?`/
/// `require_relative` all bucket `known_tail` (the exact names the census
/// histogram carried).
#[test]
fn kernel_tail_family_buckets_known_tail() {
    let (diags, recs) = dark_fixture("kernel_tail_bare_call_family_silent.rb");
    assert!(diags.is_empty(), "tail silence never diagnoses: {diags:?}");
    for m in [
        "rand",
        "caller",
        "Array",
        "URI",
        "puts",
        "sleep",
        "block_given?",
        "require_relative",
    ] {
        assert!(
            recs.iter().any(|r| *r == format!("Probe {m} known_tail")),
            "tail name {m} must bucket known_tail: {recs:?}"
        );
    }
    assert!(
        recs.iter().all(|r| !r.ends_with("closed_notfound")),
        "no tail name reaches the residue bucket: {recs:?}"
    );
}

/// A bare tail call at CLASS-BODY level runs with the class object as
/// `self` too, and buckets `known_tail` the same way.
#[test]
fn class_body_kernel_tail_buckets_known_tail() {
    let (diags, recs) = dark_fixture("class_body_kernel_tail_still_silent.rb");
    assert!(diags.is_empty(), "tail silence never diagnoses: {diags:?}");
    assert!(
        recs.iter().any(|r| r == "Boot puts known_tail"),
        "the class-body puts must bucket known_tail: {recs:?}"
    );
    assert!(
        recs.iter().all(|r| !r.ends_with("closed_notfound")),
        "no tail name reaches the residue bucket: {recs:?}"
    );
}

/// PRECEDENCE: the project defines `raise` on the class object, so the call
/// RESOLVES — the inventory is never consulted, and a resolved lookup
/// records nothing at all. The mutant this fails: consulting the tail
/// before the project's own method table (blanket suppression).
#[test]
fn project_defined_raise_resolves_and_records_nothing() {
    let (diags, recs) = dark_fixture("project_defined_raise_wins_over_inventory.rb");
    assert!(diags.is_empty(), "correct code stays silent: {diags:?}");
    assert!(
        recs.iter().all(|r| !r.starts_with("Uploader ")),
        "a project-defined raise must resolve, never bucket (let alone known_tail): {recs:?}"
    );
    assert!(
        recs.iter().all(|r| !r.ends_with("known_tail")),
        "the tail label never lands on a resolved call: {recs:?}"
    );
}

/// The accusation survives the tail: a TYPO inside a `def self.` body is
/// not a tail name, stays silent today, and remains the residue — exactly
/// what a class-object E0101 fires on once the track arms.
#[test]
fn singleton_tail_typo_stays_closed_notfound() {
    let (diags, recs) = dark_fixture("singleton_tail_typo_still_closed.rb");
    assert_eq!(
        diags.len(),
        1,
        "a non-tail typo is the residue, and the armed track accuses it: {diags:?}"
    );
    assert!(
        recs.iter().any(|r| r == "Job performm closed_notfound"),
        "a non-tail typo must stay in the residue bucket: {recs:?}"
    );
    assert_eq!(
        recs.iter().filter(|r| r.ends_with("closed_notfound")).count(),
        1,
        "exactly one residue site: {recs:?}"
    );
}

/// Bead ita-nst: the nested `def self.fetch_data` now RESOLVES, so the
/// census records nothing for it — before the filing this exact fixture
/// bucketed `Report fetch_data closed_notfound` (the red that motivated
/// the bead, measured on discourse's `EmotionDashboardReport` shape).
#[test]
fn nested_def_self_in_block_resolves_and_records_nothing() {
    let (diags, recs) = dark_fixture("nested_def_self_in_block_resolves.rb");
    assert!(diags.is_empty(), "resolution is silence: {diags:?}");
    assert!(
        recs.iter().all(|r| !r.ends_with("closed_notfound")),
        "a resolved nested def is never residue: {recs:?}"
    );
    assert!(
        recs.iter().all(|r| !r.contains(" fetch_data ")),
        "fetch_data must not appear in any record: {recs:?}"
    );
}

/// Bead ita-census: `method_return`'s nested cross-file body walk must
/// stand the census DOWN. Its byte offsets belong to `m.file`, while
/// `dark_record` attributes every record to the file under the cursor —
/// the same mismatch hover and `cast_comments` already stand down for.
/// Measured on discourse before the fix: 132k of 319k rows rendered
/// line 0 (the foreign byte is not a char boundary in the attributed
/// file), live sites appeared as exact duplicates (own walk plus every
/// host that pulled the body), and ghost sites appeared in files that
/// never call them. The host fixture here has NO census-able site of
/// its own, so any record at all in `a_recs` is a foreign-span ghost.
#[test]
fn census_foreign_body_walk_records_nothing_in_the_host() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/dark_singleton");
    let b_path = format!("{dir}/census_host_b.rb");
    let a_path = format!("{dir}/census_host_a.rb");
    let b_text = std::fs::read_to_string(&b_path).unwrap();
    let a_text = std::fs::read_to_string(&a_path).unwrap();
    let db = Db::default();
    let b = SourceFile::new(&db, b_path.into(), b_text);
    let a = SourceFile::new(&db, a_path.into(), a_text);
    ProjectFiles::new(&db, vec![a, b]);
    let (a_diags, a_recs) = check_file_dark(&db, a);
    let (b_diags, b_recs) = check_file_dark(&db, b);
    assert!(a_diags.is_empty(), "calling into b stays silent: {a_diags:?}");
    assert_eq!(
        b_diags.len(),
        1,
        "the residue is exactly one armed accusation, in its OWN file: {b_diags:?}"
    );
    assert!(
        a_recs.is_empty(),
        "the host file must record nothing from the foreign body walk: {a_recs:?}"
    );
    assert_eq!(
        b_recs
            .iter()
            .filter(|r| matches!(r.verdict, itaruby_semantic::DarkVerdict::ClosedNotFound))
            .count(),
        1,
        "exactly one residue site, emitted by its own file's walk: {b_recs:?}"
    );
}
