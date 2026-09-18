//! E0107 constraint contradiction (bead ita-dqo, deliverable 1): a proof
//! that no type — project or core — responds to every method an Unknown
//! local-variable/parameter binding was called with. Gated exactly like
//! the closed-world conclusive core lookup (bead ita-2ve): `ClosedWorld`
//! must be wired on, same as `core_conclusive.rs`'s pattern. Fixtures
//! live under `testdata/constraints/`, each with globally unique
//! (`Constr`-prefixed) class names — `testdata/` is scanned as a single
//! merged project by `ita check testdata/` (gate c).

use itaruby_semantic::{
    check_file, constraint_report, ClosedWorld, ConstraintOutcome, Db, ProjectFiles, Severity,
    SourceFile,
};

type Diags = Vec<String>;

fn check_with(name: &str, closed: bool) -> Diags {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/constraints");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap();
    let db = Db::default();
    let file = SourceFile::new(&db, path.clone().into(), text);
    ProjectFiles::new(&db, vec![file]);
    if closed {
        ClosedWorld::new(&db, true);
    }
    check_file(&db, file)
        .iter()
        .map(|d| {
            format!(
                "{}[{}]: {}",
                if d.severity == Severity::Error { "error" } else { "warning" },
                d.code,
                d.message
            )
        })
        .collect()
}

/// Check one fixture with closed-world ON.
fn check_closed(name: &str) -> Diags {
    check_with(name, true)
}

/// Check one fixture with closed-world OFF — v0/LSP behavior.
fn check_open(name: &str) -> Diags {
    check_with(name, false)
}

/// `constraint_report` over one fixture, closed-world ON (deliverable 2's
/// payload source is only ever meaningful under closed-world — see
/// `check.rs::Checker::collect_constraint`).
fn report_closed(name: &str) -> Vec<ConstraintOutcome> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/constraints");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap();
    let db = Db::default();
    let file = SourceFile::new(&db, path.clone().into(), text);
    ProjectFiles::new(&db, vec![file]);
    ClosedWorld::new(&db, true);
    constraint_report(&db, file)
}

fn has_e0107(diags: &Diags) -> bool {
    diags.iter().any(|d| d.contains("[E0107]"))
}

// -- scenario (a): core + core contradiction ---------------------------

/// `.upcase` (String-only) and `.push` (Array-only) on the same Unknown
/// binding: disjoint candidate sets, empty intersection — FIRES E0107.
/// Mutation-proof (empty-side): if `flush_constraints`'s intersection
/// were sabotaged to always return non-empty (e.g. `retain` replaced with
/// a no-op), this assertion — which requires BOTH the code AND both
/// method names named in the message — would fail.
#[test]
fn core_core_contradiction_fires() {
    let diags = check_closed("core_core_contradiction.rb");
    let hit = diags.iter().find(|d| d.contains("[E0107]"));
    let Some(hit) = hit else { panic!("expected E0107, got: {diags:?}") };
    assert!(hit.starts_with("warning["), "E0107 must be a Warning, got: {hit}");
    assert!(hit.contains(".upcase"), "message must name the conflicting methods: {hit}");
    assert!(hit.contains(".push"), "message must name the conflicting methods: {hit}");
}

/// Same fixture, closed-world OFF: E0107 must be impossible — the same
/// gate `core_unknown_is_conclusive` uses, proving `ita server`/LSP can
/// never emit it (contract: no separate LSP gating needed).
#[test]
fn core_core_contradiction_silent_without_closed_world() {
    let diags = check_open("core_core_contradiction.rb");
    assert!(!has_e0107(&diags), "no ClosedWorld input must mean no E0107, got: {diags:?}");
}

// -- scenario (b): project + project contradiction ----------------------

/// Two disjoint closed project classes, each own-defining one of the two
/// called methods — FIRES E0107.
#[test]
fn project_project_contradiction_fires() {
    let diags = check_closed("project_project_contradiction.rb");
    let hit = diags.iter().find(|d| d.contains("[E0107]"));
    let Some(hit) = hit else { panic!("expected E0107, got: {diags:?}") };
    assert!(hit.contains(".alpha_only"));
    assert!(hit.contains(".beta_only"));
}

// -- scenario (c): open class removes the only candidate -----------------

/// `gamma_only`'s only definer is `open` (via `method_missing`), so
/// `closed_candidates_for` excludes it: the call's candidate set is
/// empty, the "every method has >= 1 candidate" gate fails, and the
/// binding stays silent — even though `.upcase` alone has a fine
/// candidate. Proves invariant #1 holds through the open-class path.
#[test]
fn open_class_definer_silences() {
    let diags = check_closed("open_class_silences.rb");
    assert!(!has_e0107(&diags), "an open-class-only definer must not prove a contradiction: {diags:?}");
}

// -- scenario (d): single candidate satisfies everything -----------------

/// Both methods are only ever defined by one closed project class — the
/// intersection is exactly that class (`Inferred`, not empty) — silence.
#[test]
fn unique_candidate_silences() {
    let diags = check_closed("unique_candidate_silences.rb");
    assert!(!has_e0107(&diags), "a single satisfying candidate must not fire E0107: {diags:?}");
}

// -- scenario (e): reassignment between the two calls ---------------------

/// The binding is reassigned (to another Unknown value) between the two
/// calls: the pre-write constraint is discarded, leaving only one
/// distinct-method constraint post-write — below the >= 2 dedupe
/// threshold — silence.
#[test]
fn reassignment_between_calls_silences() {
    let diags = check_closed("reassignment_silences.rb");
    assert!(!has_e0107(&diags), "reassignment must reset the accumulator: {diags:?}");
}

// -- scenario (f): negative control, non-empty intersection --------------

/// Two closed project classes both own-define BOTH called methods: the
/// intersection has 2 members — silence. Mutation-proof (always-empty
/// side): if the intersection logic were sabotaged to always return
/// empty (e.g. `retain` replaced with `clear`), this fixture's binding
/// would wrongly fire E0107; this assertion catches exactly that.
#[test]
fn negative_control_silences() {
    let diags = check_closed("negative_control_silences.rb");
    assert!(!has_e0107(&diags), "a 2-member intersection must not fire E0107: {diags:?}");
}

// -- constraint_report: direct classification tests -----------------------

/// `Contradiction`: same proof payload the check walk's E0107 emits.
#[test]
fn report_classifies_contradiction() {
    let outcomes = report_closed("core_core_contradiction.rb");
    let contradictions: Vec<_> = outcomes
        .iter()
        .filter_map(|o| match o {
            ConstraintOutcome::Contradiction(proof) => Some(proof),
            _ => None,
        })
        .collect();
    assert_eq!(contradictions.len(), 1, "expected exactly one Contradiction, got: {outcomes:?}");
    let proof = contradictions[0];
    assert_eq!(proof.receiver, "x");
    let methods: Vec<&str> = proof.calls.iter().map(|c| c.method.as_str()).collect();
    assert_eq!(methods, vec!["upcase", "push"]);
    assert_eq!(proof.calls[0].candidates, vec!["String".to_string(), "Symbol".to_string()]);
    assert_eq!(proof.calls[1].candidates, vec!["Array".to_string()]);
}

/// `Inferred`: exactly one candidate satisfies every constraint.
#[test]
fn report_classifies_inferred() {
    let outcomes = report_closed("unique_candidate_silences.rb");
    let inferred: Vec<_> = outcomes
        .iter()
        .filter_map(|o| match o {
            ConstraintOutcome::Inferred { receiver, ty, calls } => Some((receiver, ty, calls)),
            _ => None,
        })
        .collect();
    assert_eq!(inferred.len(), 1, "expected exactly one Inferred, got: {outcomes:?}");
    let (receiver, ty, calls) = inferred[0];
    assert_eq!(receiver, "x");
    assert_eq!(ty, "ConstrUniqueDelta");
    assert_eq!(calls.len(), 2);
    assert!(outcomes
        .iter()
        .all(|o| !matches!(o, ConstraintOutcome::Contradiction(_) | ConstraintOutcome::UnionCandidate { .. })));
}

/// `UnionCandidate`: 2..=4 candidates satisfy every constraint.
#[test]
fn report_classifies_union_candidate() {
    let outcomes = report_closed("negative_control_silences.rb");
    let unions: Vec<_> = outcomes
        .iter()
        .filter_map(|o| match o {
            ConstraintOutcome::UnionCandidate { receiver, candidates, calls } => {
                Some((receiver, candidates, calls))
            }
            _ => None,
        })
        .collect();
    assert_eq!(unions.len(), 1, "expected exactly one UnionCandidate, got: {outcomes:?}");
    let (receiver, candidates, calls) = unions[0];
    assert_eq!(receiver, "x");
    assert_eq!(candidates, &vec!["ConstrSharedEpsilon".to_string(), "ConstrSharedZeta".to_string()]);
    assert_eq!(calls.len(), 2);
    assert!(!outcomes.iter().any(|o| matches!(o, ConstraintOutcome::Contradiction(_))));
}

/// A binding with >= 2 distinct method calls where one call's candidate
/// set is empty (the open-class definer, scenario c) never produces any
/// outcome at all — the same "every call has >= 1 candidate" gate that
/// keeps E0107 silent also keeps `constraint_report` from classifying it
/// as `Inferred`/`UnionCandidate`/`Contradiction`.
#[test]
fn report_ignores_untraceable_candidate_bindings() {
    let outcomes = report_closed("open_class_silences.rb");
    assert!(outcomes.is_empty(), "an untraceable-candidate binding must yield no outcome: {outcomes:?}");
}
