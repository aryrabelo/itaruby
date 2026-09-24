//! A project method's own sorbet `sig` is a CONTRACT on its body and its
//! literal arguments, but only a fallback for the CONSUMER's type. Bead
//! ita-4xy landed the declared return as a fallback consulted only when
//! body inference reached `Ty::Unknown`, and that precedence stands:
//! sorbet-runtime proves a returned value only `is_a?`, which a decorator
//! overriding it also passes, so an inferred body type always wins. The
//! body is still checked against its declaration independently, so a body
//! that disagrees is accused (E0109) where it is written. The precedence
//! assertions below are the lock on that direction.
//!
//! Inline sources only (no `testdata/` fixtures: gate c scans that whole
//! tree for diagnostics, and every fixture here is deliberately built to
//! raise one). Globally-unique class name prefix `ProjSig` per test, one
//! inline `SourceFile` per `Db`.
//!
//! Recognized-sig fixtures extend `T::Sig`, the external provider of the
//! class-body DSL. Without that provider the class-object flip correctly
//! diagnoses the bare `sig` call, independently of return inference.
//! Keep every return-type and precedence assertion unchanged; unresolved
//! extension ancestry must not hide the owner's directly defined methods.
//! The unrecognized-block control deliberately keeps no extension so its
//! silence still depends on that block opening the class, not on a mixin.

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
  extend T::Sig

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

/// (b) Precedence: the body returns a plain string literal while the sig
/// declares `ProjSigWidgetB`, and both halves must be observable from one
/// fixture:
///
/// * E0109 on `make` — the body is checked against its own declaration,
///   independently of any call site, so an implementation that contradicts
///   the signature is accused where it is written.
/// * silence on `nonexistent_method_b` — the CONSUMER is typed by the
///   inferred `Ty::Str` (a core type, `ClosedWorld` unwired here), never by
///   the declaration: a declared return fills only an Unknown.
#[test]
fn declared_return_checks_body_but_never_overrides_inference() {
    let diags = check_src(
        r#"
class ProjSigWidgetB
end

class ProjSigOwnerB
  extend T::Sig

  sig { returns(ProjSigWidgetB) }
  def make
    "just a string"
  end
end

ProjSigOwnerB.new.make.nonexistent_method_b
"#,
    );
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert!(diags[0].contains("E0109") && diags[0].contains("make"), "{diags:?}");
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
  extend T::Sig

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
  extend T::Sig

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

/// (f) Non-regression: the project's own `#:` RBS comment sig is checked
/// FIRST in `method_return`, before any body walk or sorbet sig is
/// consulted, and still wins over BOTH a disagreeing sorbet `sig { ... }`
/// AND a concretely-typed body. `make` here carries all three — an `#:`
/// sig naming `ProjSigWidgetF`, a sorbet sig naming a DIFFERENT class, and
/// a body returning a plain string — so the single E0101 on
/// `ProjSigWidgetF` (a closed leaf class) is what proves which of the
/// three governed: either other answer gives a different diagnostic or
/// none at all.
///
/// The body is deliberately NOT accused here the way (b)'s is: this
/// delivery checks a body against a sorbet `sig`, and adds no RBS
/// body-return validation. That asymmetry is a scope boundary, not an
/// oversight — an `#:` comment sig still types consumers without judging
/// the implementation underneath it.
#[test]
fn rbs_comment_sig_wins_over_sorbet_sig_and_body() {
    let diags = check_src(
        r#"
class ProjSigWidgetF
end

class ProjSigOtherSorbetF
end

class ProjSigOwnerF
  extend T::Sig

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

