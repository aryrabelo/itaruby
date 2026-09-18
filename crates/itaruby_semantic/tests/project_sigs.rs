//! Bead ita-4xy: `method_return` consumes a project method's own sorbet
//! `sig { returns(...) }` (bead ita-uh1's `MethodDef::sorbet_ret`, now
//! plumbed onto `MethodSig` too) as a FALLBACK when the method's own body
//! inference lands on `Ty::Unknown` — never a precedence override. Inline
//! sources only (no `testdata/` fixtures: gate c scans that whole tree for
//! diagnostics, and every fixture here is deliberately built to raise one).
//! Globally-unique class name prefix `ProjSig` per test, one inline
//! `SourceFile` per `Db`.

use itaruby_semantic::{check_file, Db, ProjectFiles, SourceFile};

fn check_src(text: &str) -> Vec<String> {
    let db = Db::default();
    let file = SourceFile::new(&db, "inline/project_sigs.rb".into(), text.to_string());
    ProjectFiles::new(&db, vec![file]);
    check_file(&db, file)
        .iter()
        .map(|d| format!("[{}]: {}", d.code, d.message))
        .collect()
}

/// (a) Headline case: `make`'s body is opaque (a call on an untyped
/// param — Unknown, silently, per invariant #1) but `make` carries a
/// `sig { returns(ProjSigWidgetA) }`. The fill must turn the call site's
/// `Ty::Unknown` into `Ty::Instance(ProjSigWidgetA)`, so the very next
/// call (`nonexistent_method_a`, which `ProjSigWidgetA` genuinely does not
/// define, closed leaf ancestry) fires a real E0101 — proof the fill ran.
#[test]
fn opaque_body_with_project_sig_fills_unknown_return() {
    let diags = check_src(
        r"
class ProjSigWidgetA
end

class ProjSigOwnerA
  sig { returns(ProjSigWidgetA) }
  def make(x)
    x.whatever_unknown_method
  end
end

ProjSigOwnerA.new.make(1).nonexistent_method_a
",
    );
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0101"), "expected E0101, got: {diags:?}");
    assert!(
        diags[0].contains("nonexistent_method_a"),
        "message should name the unknown method, got: {diags:?}"
    );
}

/// (b) Precedence: an inferred body type that is NOT `Ty::Unknown`
/// (here a plain string literal, `Ty::Str`) always wins over the sig —
/// `method_return`'s fallback only ever runs when the body's own answer
/// IS Unknown. If the fill wrongly stole precedence, the call site would
/// type as `Ty::Instance(ProjSigWidgetB)` instead of `Ty::Str`, and
/// `nonexistent_method_b` (real on neither, but only diagnosable on the
/// closed PROJECT class) would fire E0101. `Ty::Str` is a core type
/// (`ClosedWorld` off, unwired here), so the correct precedence stays
/// silent — this is the mutation lock for "sig steals precedence".
#[test]
fn inferred_body_beats_sig_precedence() {
    let diags = check_src(
        r#"
class ProjSigWidgetB
end

class ProjSigOwnerB
  sig { returns(ProjSigWidgetB) }
  def make
    "just a string"
  end
end

ProjSigOwnerB.new.make.nonexistent_method_b
"#,
    );
    assert!(
        diags.is_empty(),
        "an inferred `Ty::Str` body must beat the sig; if the sig stole precedence \
         this would type as ProjSigWidgetB (a closed project class) and E0101 would \
         fire; got: {diags:?}"
    );
}

/// (c) A sig naming a class that resolves nowhere in the project stays
/// `Ty::Unknown` (`resolve_ret_ty`'s own contract — never a guess), so
/// the call chain goes silent exactly like today, before this bead. The
/// sig's own unresolved name is a SEPARATE, correct E0104 (check.rs's
/// ordinary constant-reference check on the `sig { ... }` block's own
/// AST — unrelated to `method_return`, unaffected by this bead) — this
/// test asserts the return-value side specifically: no E0101 on
/// `anything_c`, proving the fallback gave up on `Ty::Unknown` rather
/// than manufacturing a project-class guess.
#[test]
fn unresolvable_sig_name_stays_unknown_and_silent() {
    let diags = check_src(
        r"
class ProjSigOwnerC
  sig { returns(ProjSigNotARealClassC) }
  def make(x)
    x.whatever_unknown_method
  end
end

ProjSigOwnerC.new.make(1).anything_c
",
    );
    assert!(
        diags.iter().all(|d| !d.contains("anything_c")),
        "an unresolvable sig name must leave the return Unknown and `anything_c` \
         silent (E0101-wise); got: {diags:?}"
    );
}

/// (d) Same as (a), but for `def self.make` (singleton track) — the
/// `Ty::Class(c) => lookup_singleton` path feeds `method_return` with
/// `singleton: true`; the fill must apply there identically.
#[test]
fn singleton_def_self_with_project_sig_fills_return() {
    let diags = check_src(
        r"
class ProjSigWidgetD
end

class ProjSigOwnerD
  sig { returns(ProjSigWidgetD) }
  def self.make(x)
    x.whatever_unknown_method
  end
end

ProjSigOwnerD.make(1).nonexistent_method_d
",
    );
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0101"), "expected E0101, got: {diags:?}");
    assert!(
        diags[0].contains("nonexistent_method_d"),
        "message should name the unknown method, got: {diags:?}"
    );
}

/// (f) Non-regression: the project's own `#:` RBS comment sig (checked
/// FIRST in `method_return`, unconditionally, before any body walk or
/// sorbet sig is even consulted) still wins over BOTH a disagreeing
/// sorbet `sig { ... }` AND a concretely-typed body — this bead changes
/// none of that existing precedence, only adds a fallback for the case
/// `#:` never covered (no RBS comment at all). `make` here has all
/// three: an `#:` sig naming `ProjSigWidgetF`, a sorbet sig naming a
/// DIFFERENT class, and a body that returns a plain string — the `#:`
/// type must be the one that governs, proven by E0101 on `ProjSigWidgetF`
/// (closed leaf class) instead of silence (which either of the other two
/// answers would give).
#[test]
fn rbs_comment_sig_wins_over_sorbet_sig_and_body() {
    let diags = check_src(
        r#"
class ProjSigWidgetF
end

class ProjSigOtherSorbetF
end

class ProjSigOwnerF
  sig { returns(ProjSigOtherSorbetF) }
  #: () -> ProjSigWidgetF
  def make
    "definitely a string"
  end
end

ProjSigOwnerF.new.make.nonexistent_method_f
"#,
    );
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0101"), "expected E0101, got: {diags:?}");
    assert!(
        diags[0].contains("nonexistent_method_f"),
        "message should name the unknown method, got: {diags:?}"
    );
}

/// (g) Control for the ita-4xy open-class carve-out itself: an
/// UNRECOGNIZED `sig { ... }` shape (here, an outermost call that is
/// neither `returns` nor `void`) must still open the class exactly like
/// before this bead — same fixture shape as (a), but the sig block is
/// unrecognized, so `ProjSigOwnerG` stays open, `make` resolves
/// Inconclusive, and the typo call goes silent. Without the carve-out's
/// `sig_block_is_recognized` gate, (a) and this test would be
/// indistinguishable; this is what proves the carve-out is narrow.
#[test]
fn unrecognized_sig_block_still_opens_class_and_stays_silent() {
    let diags = check_src(
        r"
class ProjSigWidgetG
end

class ProjSigOwnerG
  sig { some_unmodeled_call(ProjSigWidgetG) }
  def make(x)
    x.whatever_unknown_method
  end
end

ProjSigOwnerG.new.make(1).nonexistent_method_g
",
    );
    assert!(
        diags.is_empty(),
        "an unrecognized sig block must still open the class and stay silent, got: {diags:?}"
    );
}

