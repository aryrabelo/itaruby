//! Bead ita-exc: two E0104 (`unresolved constant`, Warning) false-positive
//! shapes both rooted in `DefWalker` (`crates/itaruby_semantic/src/index.rs`)
//! writing a constant somewhere `const_exists` never reads back from.
//!
//! Defect A — a constant written inside a class-body call's block (e.g.
//! `constvis_enums do ... ConstVisAlpha = "A" ... end`) is never indexed:
//! `DefWalker::walk_stmt`'s `CallNode` arm only specially recurses into
//! `define_method` (literal symbol) and recognized `sig{}` blocks; every
//! other block-taking class-body call marks the class `open` and returns
//! without walking the block body, so the constant assignment inside it is
//! dropped on the floor.
//!
//! Defect B — a top-level constant write (`ConstantWriteNode` or
//! `ConstantPathWriteNode` with `frag_idx == None`) is fed only into the
//! resolution-inert `FileDefs.consts` / `project_consts` map, never into
//! `fragments[i].consts` (the bucket `const_exists` reads), and
//! `const_exists`'s bare-name widening loop guards every iteration with
//! `if !walked.is_empty()`, so the top-level scope is structurally never
//! consulted even if it were populated.
//!
//! `resolve_const` stays lexical-only by design (its own doc comment says
//! so, and widening it risks manufacturing new E0101/E0102/E0103 per
//! invariant #1) — no test in this file ever asserts a resolved TYPE, only
//! presence or absence of E0104.
//!
//! The five mutants this fixture set must catch, verbatim from the audit:
//!   1. top-level bucket never populated -> both `toplevel_*_ref` tests fail
//!   2. `const_exists`' new terminal top-level fallback removed -> same two fail
//!   3. block-body constant recursion removed -> `block_const_def` and
//!      `block_const_qualified_ref` tests fail
//!   4. block recursion lands at top level instead of the enclosing class ->
//!      `block_const_wrong_scope_still_warns` fails
//!   5. `const_exists` returns `true` unconditionally -> both negative
//!      controls fail
//!
//! Bead ita-9he adds a third shape to defect B, in the SAME
//! `ConstantPathWriteNode` arm: a top-level constant write in cbase form
//! (`::X = v`, single segment) trims to a bare name with no `::` left, so
//! the arm's existing `full.rsplit_once("::")` branch (which only ever
//! fed the multi-segment `A::B = v` shape into `qualified_writes`) matched
//! `None` and silently dropped the write — never reaching
//! `toplevel_consts`, unlike the bare `ConstantWriteNode` arm right above
//! it. The read side has a matching gap: `const_exists`'s empty-`prefix`
//! fallback (reached only by a cbase single-segment reference) checked
//! `toplevel_consts` for the UNTRIMMED `::X` text, which no write —
//! including the fixed one — is ever keyed by.
//!
//! Two more mutants, verbatim from the audit:
//!   6. the new cbase-write harvest (index.rs's `ConstantPathWriteNode`
//!      arm, `None` branch) removed -> `cbase_write_bare_ref_visible_cross_file`
//!      and `cbase_write_cbase_ref_same_file_silent` both fail
//!   7. that harvest broadened to ALSO push a write inside a
//!      class/module body (not a genuine cbase target) into
//!      `toplevel_consts` -> `cbase_write_module_scope_control`'s
//!      `leak_check` diagnostic disappears

fn dir() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/const_visibility")
}

/// Loads every named fixture into one project (mirrors real usage: `ita
/// check testdata/` treats the whole tree as a single project) and returns
/// each file's diagnostics, in the same order as `names`, formatted as
/// `line:col CODE message`.
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
        .map(|(name, text)| itaruby_semantic::SourceFile::new(&db, format!("{dir}/{name}").into(), text.clone()))
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

/// Mutants 1 & 2: top-level scalar constant, referenced from another file.
#[test]
fn toplevel_const_scalar_visible_cross_file() {
    let mut diags = check_project(&["toplevel_const_def.rb", "toplevel_const_ref.rb"]);
    let ref_diags = diags.pop().unwrap();
    let def_diags = diags.pop().unwrap();
    assert!(def_diags.is_empty(), "the definition file itself has nothing to resolve: {def_diags:?}");
    assert!(
        ref_diags.is_empty(),
        "`CONSTVIS_LIST` is a top-level constant defined in the paired file: no E0104, got: {ref_diags:?}"
    );
}

/// Mutants 1 & 2: the `Name = Class.new(StandardError)` top-level idiom
/// (the corpus-b shape), referenced from another file.
#[test]
fn toplevel_class_new_visible_cross_file() {
    let mut diags = check_project(&["toplevel_class_new_def.rb", "toplevel_class_new_ref.rb"]);
    let ref_diags = diags.pop().unwrap();
    let def_diags = diags.pop().unwrap();
    assert!(def_diags.is_empty(), "the definition file itself has nothing to resolve: {def_diags:?}");
    assert!(
        ref_diags.is_empty(),
        "`ConstVisSentinel` is a top-level `Class.new` constant defined in the paired file: no E0104, got: {ref_diags:?}"
    );
}

/// Mutant 3: a constant written inside an arbitrary class-body call's
/// block, referenced by bare name from a method in the SAME class.
#[test]
fn block_body_const_self_reference_silent() {
    let diags = check_single("block_const_def.rb");
    assert!(
        diags.is_empty(),
        "`ConstVisAlpha`, assigned inside `constvis_enums do ... end`, must be visible to \
         `ConstVisEnum`'s own methods: no E0104, got: {diags:?}"
    );
}

/// Mutant 3: the same block-defined constant, referenced by qualified path
/// (`ConstVisEnum::ConstVisAlpha`) from a different file.
#[test]
fn block_body_const_qualified_cross_file_silent() {
    let mut diags = check_project(&["block_const_def.rb", "block_const_qualified_ref.rb"]);
    let ref_diags = diags.pop().unwrap();
    assert!(
        ref_diags.is_empty(),
        "`ConstVisEnum::ConstVisAlpha` must resolve cross-file too: no E0104, got: {ref_diags:?}"
    );
}

/// Mutants 1 & 2 again, via the synthetic `ConstVisOuter::ConstVisInner =
/// 42` qualified top-level path write (`ConstantPathWriteNode`, not
/// observed on any corpus but the same defect-B code path).
#[test]
fn nested_const_path_write_visible_cross_file() {
    let mut diags = check_project(&["nested_path_write_def.rb", "nested_path_write_ref.rb"]);
    let ref_diags = diags.pop().unwrap();
    let def_diags = diags.pop().unwrap();
    assert!(def_diags.is_empty(), "the definition file itself has nothing to resolve: {def_diags:?}");
    assert!(
        ref_diags.is_empty(),
        "`ConstVisOuter::ConstVisInner` is a top-level qualified path write: no E0104, got: {ref_diags:?}"
    );
}

/// Mutant 5 (also the sanity check every other test here depends on):
/// a genuinely undefined constant must keep warning. If this ever goes
/// silent, the fix has degraded into "the name exists somewhere" instead
/// of "the name is reachable from here".
#[test]
fn undefined_constant_still_warns() {
    let diags = check_single("undefined_still_warns.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0104"), "expected E0104, got: {:?}", diags[0]);
    assert!(
        diags[0].contains("ConstVisNeverDefinedAnywhere"),
        "message should name the unresolved constant, got: {:?}",
        diags[0]
    );
}

/// Mutants 4 & 5: `ConstVisAlpha` is defined inside `ConstVisEnum`'s
/// class-body block, but `ConstVisUnrelated` neither nests in nor
/// inherits from `ConstVisEnum`. Proves the block-body recursion the fix
/// adds records the constant against the LEXICALLY ENCLOSING class, not
/// at top level or project-wide — a real Ruby `NameError`, must still warn.
#[test]
fn block_body_const_wrong_scope_still_warns() {
    let mut diags = check_project(&["block_const_def.rb", "block_const_wrong_scope_still_warns.rb"]);
    let wrong_scope_diags = diags.pop().unwrap();
    assert_eq!(wrong_scope_diags.len(), 1, "expected exactly 1 diagnostic, got: {wrong_scope_diags:?}");
    assert!(wrong_scope_diags[0].contains("E0104"), "expected E0104, got: {:?}", wrong_scope_diags[0]);
    assert!(
        wrong_scope_diags[0].contains("ConstVisAlpha"),
        "message should name the unresolved constant, got: {:?}",
        wrong_scope_diags[0]
    );
}

/// Mutant 6 (fixture a): cbase top-level write (`::ConstVisCbaseDb = ...`)
/// referenced by bare name from a class method in a different file.
#[test]
fn cbase_write_bare_ref_visible_cross_file() {
    let mut diags = check_project(&["cbase_write_def.rb", "cbase_write_bare_ref.rb"]);
    let ref_diags = diags.pop().unwrap();
    let def_diags = diags.pop().unwrap();
    assert!(def_diags.is_empty(), "the definition file itself has nothing to resolve: {def_diags:?}");
    assert!(
        ref_diags.is_empty(),
        "`ConstVisCbaseDb` is a top-level cbase write: no E0104, got: {ref_diags:?}"
    );
}

/// Mutant 6 (fixture b): the measured `rails_shape.rb` regression shape —
/// a cbase write AND a cbase read of the SAME constant in ONE file.
#[test]
fn cbase_write_cbase_ref_same_file_silent() {
    let diags = check_single("cbase_write_cbase_ref_same_file.rb");
    assert!(
        diags.is_empty(),
        "`::ConstVisCbaseFiles` is written and read via cbase in the same file: no E0104, got: {diags:?}"
    );
}

/// Mutant 7 (fixture c): a top-level cbase write must resolve
/// (`ConstVisCbaseShared`), and a module's own bare constant
/// (`ConstVisCbaseModuleOnly`) must NOT leak into `toplevel_consts` — an
/// unrelated class's bare reference to it must still warn.
#[test]
fn cbase_write_module_scope_control() {
    let diags = check_single("cbase_write_module_scope_control.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0104"), "expected E0104, got: {:?}", diags[0]);
    assert!(
        diags[0].contains("ConstVisCbaseModuleOnly"),
        "the module-scoped constant must still warn from outside its module (no leak into \
         toplevel_consts), got: {:?}",
        diags[0]
    );
}

/// Fixture d (also a mutant-6 guard on the read side): a cbase reference
/// to a constant that is never written anywhere must still warn E0104.
#[test]
fn cbase_negative_control_still_warns() {
    let diags = check_single("cbase_negative_control_still_warns.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0104"), "expected E0104, got: {:?}", diags[0]);
    assert!(
        diags[0].contains("ConstVisCbaseNeverDefinedAnywhere"),
        "message should name the unresolved constant, got: {:?}",
        diags[0]
    );
}
/// Mutant 8 (M3 keying control): the unresolved-owner fallback must key
/// `toplevel_consts` by the FULL path, never the bare simple segment — a
/// same-named top-level write must NOT rescue `ConstVisMissing::ConstVisM3Inner`.
/// Was a BLIND mutant before this fixture existed (always-`simple` passed 11/11).
#[test]
fn m3_simple_key_control_still_warns() {
    let diags = check_single("m3_simple_key_control_still_warns.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0104"), "expected E0104, got: {:?}", diags[0]);
    assert!(
        diags[0].contains("ConstVisMissing::ConstVisM3Inner")
            || diags[0].contains("ConstVisM3Inner"),
        "the unresolved-owner read must warn despite the same-named top-level write, got: {:?}",
        diags[0]
    );
}
