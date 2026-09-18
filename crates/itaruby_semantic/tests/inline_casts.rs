//! Bead ita-qst: `expr #: as Type` inline type-assertion comments
//! (sorbet.org/docs/rbs-support), the form `rbs_comment.rs`'s
//! `parse_rbs_comment` never covered (that parser only ever owns the
//! METHOD-signature form, `#: (params) -> ret`). Measured FP: ruby-lsp's
//! `lib/ruby_indexer/lib/ruby_indexer/index.rb:660` —
//! `index_single(uri, source, #: as !nil)` — leaves `source` typed
//! `String | nil` against `index_single`'s own `(String) -> void` sig,
//! firing a false E0103. `check.rs::Checker::apply_cast_comment` binds a
//! trailing `#: as <target>` comment to the ONE call argument sharing its
//! physical line and retypes it for that call's arg-type check only.
//!
//! MUTANTS THIS FILE MUST CATCH:
//!   1. Cast-comment recognition disabled (`apply_cast_comment` made a
//!      no-op, or `collect_cast_comments` never wired into `check_call`)
//!      — `ruby_lsp_shape_stays_silent` flips to a real E0103.
//!   2. `!nil` stripping applied to `Ty::Unknown` (i.e. `strip_nil`
//!      swapped for something that turns Unknown into a concrete type,
//!      or `apply_cast_comment`'s `NotNil` arm bypasses `strip_nil`
//!      entirely and invents a type) — `not_nil_leaves_unknown_unknown`
//!      fails.
//!   3. The cast comment's line-binding leaks to the NEXT physical line
//!      (e.g. `cast_comment_at` matches by file position instead of
//!      line-range containment, or the collected range spans past its
//!      own line) — `multiline_call_second_arg_still_accuses` drops back
//!      to silence.

use itaruby_semantic::{check_file, Db, ProjectFiles, SourceFile};

fn check_fixture(name: &str) -> Vec<String> {
    let base = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/inline_casts");
    let path = format!("{base}/{name}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let db = Db::default();
    let file = SourceFile::new(&db, format!("/p/{name}").into(), text);
    ProjectFiles::new(&db, vec![file]);
    check_file(&db, file)
        .iter()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect()
}

/// A local variable declared `?String? source` cast `#: as !nil` on the
/// exact ruby-lsp shape satisfies a callee's required `String` param —
/// zero E0103 (and zero diagnostics at all).
#[test]
fn ruby_lsp_shape_stays_silent() {
    let diags = check_fixture("ruby_lsp_shape_silent.rb");
    assert!(diags.is_empty(), "expected zero diagnostics, got: {diags:?}");
}

/// `!nil` narrows a real `Nil`/`Union` — it must never *invent* a type
/// out of `Ty::Unknown`. A param with no RBS sig (Unknown by
/// construction) cast `#: as !nil` and passed to a sig expecting a
/// concrete class stays silent (Unknown always satisfies `compatible`),
/// proving the cast left it Unknown rather than becoming some guessed
/// concrete type that would ALSO happen to satisfy the sig for the wrong
/// reason. A companion negative: if `!nil` ever fabricated a type, the
/// arg would resolve to something other than Unknown, and this same call
/// would remain silent for a reason invariant #1 forbids — so this test
/// alone cannot distinguish "narrowed correctly" from "silenced for the
/// wrong reason"; `not_nil_on_unknown_param_never_narrows_away_a_real_mismatch`
/// below closes that gap directly against `strip_nil`'s own contract.
#[test]
fn not_nil_leaves_unknown_unknown() {
    let src = "\
class InlineCastNilUnknownSink
end

class InlineCastNilUnknownWrong
end

class InlineCastNilUnknownUser
  #: (InlineCastNilUnknownWrong) -> void
  def take(target)
  end
end

def inline_cast_nil_unknown_case(unknown_param)
  InlineCastNilUnknownUser.new.take(
    unknown_param, #: as !nil
  )
end
";
    let db = Db::default();
    let file = SourceFile::new(&db, "/p/nil_unknown.rb".into(), src.to_string());
    ProjectFiles::new(&db, vec![file]);
    let diags = check_file(&db, file);
    assert!(
        diags.is_empty(),
        "an Unknown param cast `!nil` must stay Unknown (still compatible with any param), \
         got: {diags:?}"
    );
}

/// Direct contract test against `strip_nil` itself (not the integration
/// path): stripping `Nil` out of `Ty::Union([Str, Nil])` yields `Str`,
/// never something wider. This is the pure-function half mutant (2)
/// targets; the fixture above is the end-to-end half.
#[test]
fn not_nil_on_unknown_param_never_narrows_away_a_real_mismatch() {
    // A local var whose real (non-Unknown) type is a Union containing Nil
    // must keep firing E0103 when cast to a class that plainly is not
    // the non-nil half of that union — proving `!nil` only ever strips
    // `Nil`, never coerces to an unrelated class.
    let src = "\
class InlineCastNilRealSink
end

class InlineCastNilRealUser
  #: (InlineCastNilRealSink) -> void
  def take(target)
  end

  #: (?String? source) -> void
  def call_it(source = nil)
    InlineCastNilRealUser.new.take(
      source, #: as !nil
    )
  end
end
";
    let db = Db::default();
    let file = SourceFile::new(&db, "/p/nil_real.rb".into(), src.to_string());
    ProjectFiles::new(&db, vec![file]);
    let diags: Vec<String> = check_file(&db, file)
        .iter()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect();
    assert!(
        diags.iter().any(|d| d.starts_with("E0103")),
        "`!nil` on a real `String | nil` must strip Nil to `String`, not coerce to \
         `InlineCastNilRealSink` — expected E0103, got: {diags:?}"
    );
}

/// `#: as Foo` retypes a genuinely mismatched argument to the real
/// project class it names — zero E0103.
#[test]
fn class_cast_makes_mismatched_arg_compatible() {
    let diags = check_fixture("class_cast_silent.rb");
    assert!(diags.is_empty(), "expected zero diagnostics, got: {diags:?}");
}

/// `#: as Blah` (no project class named `Blah`) must never invent a
/// type — the argument keeps its real type and the existing mismatch
/// still fires exactly as it would with no cast comment at all.
#[test]
fn garbage_cast_stays_byte_identical_to_no_cast() {
    let diags = check_fixture("garbage_cast_still_accuses.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].starts_with("E0103"), "expected E0103, got: {diags:?}");
    assert!(
        diags[0].contains("InlineCastFooC") && diags[0].contains("InlineCastWrongC"),
        "expected the untouched original mismatch (Foo vs Wrong), got: {diags:?}"
    );
}

/// A cast that resolves fine to a REAL class, but still doesn't match
/// the callee's own declared param type, must keep accusing — the cast
/// retypes the one argument, it does not blanket-silence the call.
#[test]
fn cast_to_real_class_still_wrong_for_callee_accuses() {
    let diags = check_fixture("still_invalid_after_cast_accuses.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].starts_with("E0103"), "expected E0103, got: {diags:?}");
    assert!(
        diags[0].contains("InlineCastOtherD") && diags[0].contains("InlineCastFooD"),
        "expected the sig's required type vs. the cast's actual target, got: {diags:?}"
    );
}

/// The cast comment binds to the ONE argument on its own physical line —
/// a sibling argument on the NEXT line of the same multi-line call must
/// keep its real (mismatched) type: exactly 1 E0103, for the second
/// argument only.
#[test]
fn multiline_call_second_arg_still_accuses() {
    let diags = check_fixture("multiline_no_leak_accuses.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic (arg 2 only), got: {diags:?}");
    assert!(diags[0].starts_with("E0103"), "expected E0103, got: {diags:?}");
    assert!(
        diags[0].contains("argument 2"),
        "the leaking cast must not have silenced argument 2, got: {diags:?}"
    );
    assert!(
        diags[0].contains("InlineCastFooE") && diags[0].contains("InlineCastWrongE"),
        "expected arg 2's real (uncast) mismatch, got: {diags:?}"
    );
}

/// Bead ita-j0z: `#: as untyped` erases the argument's type
/// (`Ty::Unknown`) rather than leaving it untouched — zero diagnostics
/// when a genuinely wrong-typed value is cast `as untyped` into a sink
/// declaring a real class param.
#[test]
fn untyped_cast_erases_arg_silent() {
    let diags = check_fixture("untyped_cast_erases_arg_silent.rb");
    assert!(diags.is_empty(), "expected zero diagnostics, got: {diags:?}");
}

/// Control: a capitalized, unresolvable class name that merely LOOKS
/// like it could mean "erase the type" (but isn't the literal `untyped`
/// token) must still go through the existing `Named` arm — the
/// argument's real mismatch keeps accusing exactly as it would with no
/// cast at all.
#[test]
fn untyped_lookalike_garbage_class_still_accuses() {
    let diags = check_fixture("untyped_lookalike_garbage_class_still_accuses.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].starts_with("E0103"), "expected E0103, got: {diags:?}");
}

/// The `#: as untyped` cast binds to the ONE argument sharing its
/// physical line — a sibling argument on a different line of the same
/// multi-line call must keep its real (mismatched) type. Mutant (b):
/// applying the cast to the whole line/call instead of the one argument
/// would silence this sibling too.
#[test]
fn untyped_cast_no_leak_sibling_accuses() {
    let diags = check_fixture("untyped_cast_no_leak_sibling_accuses.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic (arg 2 only), got: {diags:?}");
    assert!(diags[0].starts_with("E0103"), "expected E0103, got: {diags:?}");
    assert!(
        diags[0].contains("argument 2"),
        "the leaking cast must not have silenced argument 2, got: {diags:?}"
    );
}

/// Control: a genuine mismatch with NO cast comment at all must still
/// accuse — proves the new `Untyped` variant didn't widen silence
/// beyond lines that actually carry `#: as untyped`.
#[test]
fn no_cast_real_mismatch_still_accuses() {
    let diags = check_fixture("no_cast_real_mismatch_still_accuses_e0z.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].starts_with("E0103"), "expected E0103, got: {diags:?}");
}

/// Real ruby-lsp shape (`test/addon_test.rb:149`): `x = <call> #: as
/// untyped` on the ASSIGNMENT's own line, not a call argument. The
/// local's type must be erased to `Ty::Unknown`, silencing the
/// otherwise-unknown `.settings` method call entirely.
#[test]
fn untyped_cast_assignment_silences_method_call() {
    let diags = check_fixture("untyped_cast_assignment_silences_method_call.rb");
    assert!(diags.is_empty(), "expected zero diagnostics, got: {diags:?}");
}

/// Control: the identical assignment shape with no cast comment at all
/// must still accuse E0101 on the unknown method.
#[test]
fn no_cast_assignment_unknown_method_still_accuses() {
    let diags = check_fixture("no_cast_assignment_unknown_method_still_accuses.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].starts_with("E0101"), "expected E0101, got: {diags:?}");
}
