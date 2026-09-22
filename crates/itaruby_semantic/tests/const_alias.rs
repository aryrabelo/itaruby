//! Bead ita-54k: `X = Y` (a bare `ConstantWriteNode`, or qualified `A::B =
//! C::D` via `ConstantPathWriteNode`) where the RHS is ITSELF a literal
//! constant path is a constant ALIAS — the real ruby-lsp shape
//! (`RubyLsp::Constant = LanguageServer::Protocol::Constant`, confirmed
//! against the gem's own vendorized RBI, found in bead ita-40k). Before
//! this bead, `const_exists`'s qualified branch only ever tried
//! `resolve_const` on the written prefix; an alias's LHS is never a real
//! class/module (no `class`/`module` keyword created it — it's a plain
//! assignment), so a reference NESTED through the alias
//! (`ConstAliasSimpleBridge::NESTED`) always fell through to
//! `toplevel_consts`/`stdlib_declares` and warned a false E0104 — even
//! though a bare reference to the alias name ALONE (no `::` suffix)
//! already resolved trivially (any assignment registers its own name,
//! regardless of what the RHS is).
//!
//! `ProjectIndex::const_aliases` records the write-site scope and literal
//! RHS. Class resolution and constant existence share a bounded path walk,
//! reevaluating each RHS in that scope. Only proven identities become types;
//! cycles, exhausted hop budgets, and conflicting ancestry remain unknown.
//! Plain value constants retain their independent membership checks.
//!
//! These fixtures distinguish valid alias resolution from genuine missing
//! members/non-literal RHSs. `ancestry_review_controls` and the isolated
//! ancestry mutant harness additionally exercise per-hop scope barriers,
//! lexical-alias precedence, cycles, and the shared traversal budget.

fn dir() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/const_alias")
}

/// Loads every named fixture into one project (mirrors `ita check
/// testdata/`: the whole tree is one project) and returns each file's
/// diagnostics, in the same order as `names`, formatted as `line:col CODE
/// message`.
fn check_project(names: &[&str]) -> Vec<Vec<String>> {
    let dir = dir();
    let db = itaruby_semantic::Db::default();
    let texts: Vec<String> = names
        .iter()
        .map(|name| std::fs::read_to_string(format!("{dir}/{name}")).unwrap())
        .collect();
    let files: Vec<itaruby_semantic::SourceFile> = names
        .iter()
        .zip(texts.iter())
        .map(|(name, text)| {
            itaruby_semantic::SourceFile::new(&db, format!("{dir}/{name}").into(), text.clone())
        })
        .collect();
    itaruby_semantic::ProjectFiles::new(&db, files.clone());
    files
        .iter()
        .zip(texts.iter())
        .map(|(file, text)| {
            let li = itaruby_semantic::LineIndex::new(text);
            itaruby_semantic::check_file(&db, *file)
                .iter()
                .map(|d| {
                    let (l, c) = li.line_col(text, d.start);
                    format!("{}:{}:{} {}", l + 1, c + 1, d.code, d.message)
                })
                .collect()
        })
        .collect()
}

fn check_single(name: &str) -> Vec<String> {
    check_project(&[name]).into_iter().next().unwrap()
}

/// Mutant 1: bare alias (`ConstAliasSimpleBridge = ConstAliasSimpleTarget`),
/// referenced with a nested suffix from a different file.
#[test]
fn simple_bare_alias_nested_access_resolves() {
    let mut diags = check_project(&["simple_alias_def.rb", "simple_alias_ref.rb"]);
    let ref_diags = diags.pop().unwrap();
    let def_diags = diags.pop().unwrap();
    assert!(
        def_diags.is_empty(),
        "the definition file itself has nothing to resolve: {def_diags:?}"
    );
    assert!(
        ref_diags.is_empty(),
        "`ConstAliasSimpleBridge::CONST_ALIAS_SIMPLE_NESTED` must resolve exactly as \
         `ConstAliasSimpleTarget::CONST_ALIAS_SIMPLE_NESTED` would: no E0104, got: {ref_diags:?}"
    );
}

/// Mutant 1: qualified-LHS alias (`ConstAliasQualOwner::Bridge =
/// ConstAliasQualTarget`), referenced with a nested suffix from a
/// different file.
#[test]
fn qualified_alias_nested_access_resolves() {
    let mut diags = check_project(&["qualified_alias_def.rb", "qualified_alias_ref.rb"]);
    let ref_diags = diags.pop().unwrap();
    let def_diags = diags.pop().unwrap();
    assert!(
        def_diags.is_empty(),
        "the definition file itself has nothing to resolve: {def_diags:?}"
    );
    assert!(
        ref_diags.is_empty(),
        "`ConstAliasQualOwner::Bridge::CONST_ALIAS_QUAL_NESTED` must resolve exactly as \
         `ConstAliasQualTarget::CONST_ALIAS_QUAL_NESTED` would: no E0104, got: {ref_diags:?}"
    );
}

/// Mutant 2: a cyclic alias chain must never hang or panic. Under the
/// shared tri-state walk a cycle is INCONCLUSIVE, not a proven miss, so a
/// reference through it is suppression-only (no E0104) — the `Ambiguous`
/// arm of `ConstResolution`, never fabricating a type either. The
/// genuine-missing controls below still accuse, so this silence is scoped
/// to real uncertainty, not a blanket suppressor.
#[test]
fn alias_cycle_degrades_to_silent_miss_without_panic() {
    let diags = check_single("cycle_alias_silent.rb");
    assert!(
        diags.is_empty(),
        "a cyclic alias is inconclusive, not a proven miss: expected no diagnostic, got: {diags:?}"
    );
}

/// Mutant 3: an alias to a name that plainly doesn't exist anywhere must
/// never be widened into a hit. Two diagnostics are genuinely correct
/// here, not a mutant symptom: the alias's OWN right-hand side
/// (`ConstAliasNeverDefinedAnywhere`) is a real, separate unresolved
/// reference at the assignment site (`check.rs`'s ordinary
/// `ConstantWriteNode` value walk), unrelated to whether the alias chase
/// itself fabricates a suppression for the NESTED reference.
#[test]
fn alias_to_undefined_name_stays_a_genuine_miss() {
    let diags = check_single("alias_to_undefined_silent.rb");
    assert_eq!(diags.len(), 2, "expected exactly 2 diagnostics, got: {diags:?}");
    assert!(
        diags.iter().all(|d| d.contains("E0104")),
        "expected only E0104, got: {diags:?}"
    );
    assert!(
        diags.iter().any(|d| d.contains("ConstAliasNeverDefinedAnywhere")),
        "the alias's own RHS is a genuinely unresolved reference too, got: {diags:?}"
    );
    assert!(
        diags.iter().any(|d| d.contains("ConstAliasToUndefined::WHATEVER")),
        "message should name the unresolved nested reference, got: {diags:?}"
    );
}

/// Mutant 3 (also the sanity check every other test here depends on): a
/// genuinely undefined nested constant, with no alias involved at all,
/// must keep warning.
#[test]
fn no_alias_negative_control_still_warns() {
    let diags = check_single("control_no_alias_still_warns.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0104"), "expected E0104, got: {:?}", diags[0]);
    assert!(
        diags[0].contains("ConstAliasControlUndefined::NESTED"),
        "message should name the unresolved constant, got: {:?}",
        diags[0]
    );
}

// ---------------------------------------------------------------------------
// Bead ita-47y: member resolution THROUGH a followed alias, arbitrarily
// many `::`-segments past the alias itself — the actual measured
// ruby-lsp shape (`Interface = LanguageServer::Protocol::Interface`, then
// `Interface::CompletionItemKind::FIELD`, TWO segments past the alias).
// Bead ita-54k above only ever chased ONE segment past an alias
// (`const_exists`'s qualified branch splits on the FINAL `::` only, so
// only a prefix that IS the alias's exact written LHS ever matched); a
// second `::`-segment past the alias fell straight back to a false
// E0104, unfixed until now.
//
// Fix (`crates/itaruby_semantic/src/index.rs`):
// `ProjectIndex::resolve_const_through_aliases` walks a path one
// `::`-segment at a time, letting an alias resolve ANY segment (not only
// the text immediately left of the final `::`) via
// `resolve_alias_segment`, and — unlike the suppression-only
// `resolve_const_via_alias` — returns a real `ClassId`, so
// `check.rs::infer_const` can type the reference `Ty::Class(id)` exactly
// like a direct reference to the target would. That is what lets a
// wrong member reached through the alias legitimately raise
// E0101/E0103, not just silence E0104.
//
// MUTANTS THIS SECTION MUST CATCH:
//   (a) alias-following past the first extra segment disabled (e.g.
//       `resolve_const_through_aliases` collapsed back to
//       `resolve_const_via_alias`'s single-hop behavior) ->
//       `ruby_lsp_shape_bare_two_segments_past_alias_resolves` and
//       `ruby_lsp_shape_qualified_namespace_resolves` both fail, the
//       false E0104 comes back.
//   (b) the cycle guard `resolve_alias_segment`/
//       `resolve_const_through_aliases` relies on (inherited from
//       `resolve_const_via_alias`) removed -> `cycle_nested_ref.rb`
//       hangs instead of completing — caught by this whole test binary
//       finishing at all, and enforced explicitly with a bounded-time
//       run in `alias_cycle_through_nested_access_never_hangs`.
//   (c) alias resolution treated as suppression-only past the alias
//       (member existence never actually checked once an alias is
//       involved) -> `control_member_*` tests wrongly go silent on a
//       member that genuinely does not exist.

/// Mutant (a): bare alias, referenced TWO segments deep
/// (`Interface::CompletionItemKind::FIELD`) — the exact ruby-lsp shape.
#[test]
fn ruby_lsp_shape_bare_two_segments_past_alias_resolves() {
    let diags = check_project(&["ruby_lsp_shape_def.rb", "ruby_lsp_shape_ref.rb"]);
    let ref_diags = &diags[1];
    assert!(
        ref_diags.is_empty(),
        "`ConstAlias47yInterface::CompletionItemKind::FIELD` must resolve exactly as \
         `ConstAlias47yProtocol::Interface::CompletionItemKind::FIELD` would: no E0104, got: {ref_diags:?}"
    );
}

/// Mutant (a): qualified-LHS alias defined INSIDE a project module,
/// referenced from OUTSIDE through that module's own qualifier — the
/// `RubyLsp::Constant::CompletionItemKind::FIELD` shape.
#[test]
fn ruby_lsp_shape_qualified_namespace_resolves() {
    let diags = check_project(&["ruby_lsp_shape_def.rb", "ruby_lsp_shape_ref.rb"]);
    let ref_diags = &diags[1];
    assert!(
        ref_diags.is_empty(),
        "`ConstAlias47yRubyLsp::Constant::CompletionItemKind::FIELD` must resolve exactly as \
         `ConstAlias47yProtocol::Constant::CompletionItemKind::FIELD` would: no E0104, got: {ref_diags:?}"
    );
}

/// Mutant (a): a chained alias (`A = B; B = C`, `C` real) referenced with
/// an extra segment past the first hop.
#[test]
fn chained_alias_nested_two_segments_resolves() {
    let diags = check_project(&["chain_nested_def.rb", "chain_nested_ref.rb"]);
    let ref_diags = &diags[1];
    assert!(
        ref_diags.is_empty(),
        "`ConstAlias47yChainA::Deep::VALUE` must resolve exactly as \
         `ConstAlias47yChainTarget::Deep::VALUE` would: no E0104, got: {ref_diags:?}"
    );
}

/// Mutant (c): a resolved alias must never suppress a genuinely wrong
/// member reached through it — a missing nested constant still warns
/// E0104, AND a missing method call on the resolved class now warns
/// E0101 (the "unlock" half of this bead: the receiver is no longer
/// `Ty::Unknown`, so its unknown method is conclusively diagnosable).
#[test]
fn control_member_through_alias_still_accuses() {
    let diags = check_project(&["control_member_def.rb", "control_member_ref.rb"]);
    let ref_diags = &diags[1];
    assert_eq!(
        ref_diags.len(),
        2,
        "expected exactly 2 diagnostics (bad const + bad method), got: {ref_diags:?}"
    );
    assert!(
        ref_diags.iter().any(|d| d.contains("E0104") && d.contains("NoSuchConst")),
        "missing nested constant through a resolved alias must still warn E0104, got: {ref_diags:?}"
    );
    assert!(
        ref_diags.iter().any(|d| d.contains("E0101") && d.contains("no_such_method")),
        "unlocked member resolution: a bogus method call on a class reached ONLY through the \
         alias must now raise E0101 (previously impossible: the receiver typed Ty::Unknown), \
         got: {ref_diags:?}"
    );
}

/// A cyclic alias reached via a multi-segment reference is inconclusive:
/// it proves neither a class identity nor absence. Bound the probe so a
/// cycle-guard regression cannot hang CI.
#[test]
fn alias_cycle_through_nested_access_never_hangs() {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let diags = check_project(&["cycle_nested_def.rb", "cycle_nested_ref.rb"]);
        let _ = tx.send(diags);
    });
    let diags = rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("cyclic alias through a nested reference must not hang");
    let ref_diags = &diags[1];
    assert!(ref_diags.is_empty(), "cyclic alias uncertainty must not emit E0104: {ref_diags:?}");
}

/// Negative control: a non-literal RHS (`X = SomeCall.call`, `X = 42`) is
/// NEVER treated as an alias — a `::`-suffixed reference through either
/// keeps warning E0104, byte-identical to before this bead. Guards
/// against ita-exc's "do not widen what counts as an alias" contract.
#[test]
fn non_literal_rhs_is_never_an_alias() {
    let diags = check_project(&["non_literal_control_def.rb", "non_literal_control_ref.rb"]);
    let ref_diags = &diags[1];
    assert_eq!(ref_diags.len(), 2, "expected exactly 2 diagnostics, got: {ref_diags:?}");
    assert!(
        ref_diags.iter().any(|d| d.contains("E0104") && d.contains("ConstAlias47yDynamicCall::SOMETHING")),
        "a call-valued write must never be chased as an alias, got: {ref_diags:?}"
    );
    assert!(
        ref_diags.iter().any(|d| d.contains("E0104") && d.contains("ConstAlias47yLiteralNumber::SOMETHING")),
        "a literal-valued write must never be chased as an alias, got: {ref_diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Bead ita-47y (RBI-target extension): the corpus-measured gap the fixture
// tests above did not close. `ProjectIndex::resolve_const_through_aliases`
// only ever chases an alias into a REAL PROJECT `ClassId` — but ruby-lsp's
// actual `Interface = LanguageServer::Protocol::Interface` aliases a
// target that lives ONLY in a client's vendorized
// `sorbet/rbi/gems/language_server-protocol@*.rbi`, never as project code.
// Against the real corpus this landed ZERO effect (449 -> 446 E0104s):
// every alias chase died at the first `by_path` miss on the target text.
//
// Fix (`crates/itaruby_semantic/src/index.rs`):
// `ProjectIndex::expand_unresolved_alias_target` walks the SAME
// segment-by-segment discipline as `resolve_const_through_aliases`, but the
// moment a segment is BOTH unresolved in-project AND itself a literal
// alias, chases that alias to its raw leaf TEXT
// (`chase_alias_target_text`) instead of giving up — handing the expanded
// path to `Checker::check_const_ref`, which retries it against
// `rbi_declares` (class/module) and the new `rbi_qualified_const_declares`
// (a qualified `Owner::Simple = T.let(...)` write, the real shape a
// vendorized gem RBI uses for enum members). Suppression-only, exactly
// like every other RBI channel: `infer_const` never calls this path, so a
// hit here can never produce a `Ty` or unlock E0101/E0103 — invariant #1
// stays intact by construction, not by discipline.
//
// MUTANT THIS SECTION MUST CATCH:
//   alias-to-RBI expansion disabled (`expand_unresolved_alias_target`
//   mutated to always return `None`) -> both `*_resolves` tests below fail,
//   re-emitting the exact false E0104 the corpus measured. The control
//   assertion inside `rbi_target_bare_alias_resolves` (`bad`'s `NOPE`)
//   catches a "treat any expansion as a hit" mutant on its own.

/// Loads fixtures `names` PLUS this dir's own toy `sorbet/rbi` (bead
/// ita-47y's RBI-target extension), mirroring `tests/rbi.rs`'s
/// `check_fixture` harness. Separate from `check_project` above: every
/// OTHER fixture in this file needs pure project-side alias resolution
/// with NO `RbiProject` wired at all — only this bead's RBI extension
/// needs a vendorized gem declaration to chase an alias INTO.
fn check_project_with_rbi(names: &[&str]) -> Vec<Vec<String>> {
    let dir = dir();
    let db = itaruby_semantic::Db::default();
    let texts: Vec<String> = names
        .iter()
        .map(|name| std::fs::read_to_string(format!("{dir}/{name}")).unwrap())
        .collect();
    let files: Vec<itaruby_semantic::SourceFile> = names
        .iter()
        .zip(texts.iter())
        .map(|(name, text)| {
            itaruby_semantic::SourceFile::new(&db, format!("{dir}/{name}").into(), text.clone())
        })
        .collect();
    itaruby_semantic::ProjectFiles::new(&db, files.clone());

    let rbi_dir = std::path::Path::new(dir).join("sorbet/rbi");
    let rbi_files = itaruby_semantic::rbi::discover_rbi_files(&rbi_dir);
    assert!(!rbi_files.is_empty(), "fixture must carry at least one .rbi file");
    let index = itaruby_semantic::rbi::build_rbi_index(&rbi_files);
    itaruby_semantic::RbiProject::new(&db, index.constants);

    files
        .iter()
        .zip(texts.iter())
        .map(|(file, text)| {
            let li = itaruby_semantic::LineIndex::new(text);
            itaruby_semantic::check_file(&db, *file)
                .iter()
                .map(|d| {
                    let (l, c) = li.line_col(text, d.start);
                    format!("{}:{}:{} {}", l + 1, c + 1, d.code, d.message)
                })
                .collect()
        })
        .collect()
}

/// Bare alias whose target is RBI-only, referenced two segments deep —
/// the exact real ruby-lsp shape.
#[test]
fn rbi_target_bare_alias_resolves() {
    let diags = check_project_with_rbi(&["rbi_target_def.rb", "rbi_target_ref.rb"]);
    let ref_diags = &diags[1];
    assert_eq!(
        ref_diags.len(),
        1,
        "expected exactly 1 diagnostic (only `bad` should accuse), got: {ref_diags:?}"
    );
    assert!(
        ref_diags[0].contains("E0104") && ref_diags[0].contains("NOPE"),
        "`read` must resolve silently through the vendorized RBI (`FIELD` is declared there); \
         `bad`'s genuinely undeclared `NOPE` must still accuse, got: {ref_diags:?}"
    );
}

/// Qualified-LHS alias inside a project namespace whose target is
/// RBI-only, referenced from outside through the namespace's own
/// qualifier — the other measured ruby-lsp shape
/// (`RubyLsp::Constant::...`).
#[test]
fn rbi_target_qualified_namespace_alias_resolves() {
    let diags = check_project_with_rbi(&["rbi_target_namespace_def.rb", "rbi_target_namespace_ref.rb"]);
    let ref_diags = &diags[1];
    assert!(
        ref_diags.is_empty(),
        "`ConstAlias47yRbiNamespace::Interface::CompletionItemKind::FIELD` must resolve through \
         the vendorized RBI exactly as a direct reference to the target would: no E0104, got: {ref_diags:?}"
    );
}
