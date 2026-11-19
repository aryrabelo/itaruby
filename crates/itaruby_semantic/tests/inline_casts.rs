//! Bead ita-qst: `expr #: as Type` inline type-assertion comments
//! (sorbet.org/docs/rbs-support), the form `rbs_comment.rs`'s
//! `parse_rbs_comment` never covered (that parser only ever owns the
//! METHOD-signature form, `#: (params) -> ret`). Measured FP: ruby-lsp's
//! `lib/ruby_indexer/lib/ruby_indexer/index.rb:660` —
//! `index_single(uri, source, #: as !nil)` — leaves `source` typed
//! `String | nil` against `index_single`'s own `(String) -> void` sig,
//! firing a false E0103. `check.rs::Checker::apply_cast_comment` binds a
//! trailing `#: as <target>` comment to the ONE call argument (or local
//! assignment) sharing its physical line and retypes it there only.
//!
//! Since MATRIX H2 the `#:` sig judges only arguments WRITTEN as literals,
//! and a literal behind a cast is never judged, so a cast's retyping is
//! read through hover on a cast local assignment, and each accusing
//! fixture pairs a silent non-literal call with a literal one.
//!
//! MUTANTS THIS FILE MUST CATCH:
//!   1. Cast-comment recognition disabled (`apply_cast_comment` made a
//!      no-op, or `collect_cast_comments` never wired into `check_call`)
//!      — the cast `:first` literal in `multiline_call_second_arg_still_accuses`
//!      is judged, a second E0103.
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

/// The type hover reports for the first `at` after the first `anchor` in
/// `text` (`None` is `Ty::Unknown`).
fn hover_in(name: &str, text: &str, anchor: &str, at: &str) -> Option<String> {
    let base = text.find(anchor).unwrap_or_else(|| panic!("{anchor:?} not in {name}"));
    let offset = base + text[base..].find(at).unwrap_or_else(|| panic!("{at:?} not after {anchor:?}"));
    let db = Db::default();
    let file = SourceFile::new(&db, format!("/p/{name}").into(), text.to_owned());
    ProjectFiles::new(&db, vec![file]);
    itaruby_semantic::hover_at(&db, file, offset).and_then(|h| h.ty)
}

fn hover_fixture(name: &str, anchor: &str, at: &str) -> Option<String> {
    let base = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/inline_casts");
    let path = format!("{base}/{name}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    hover_in(name, &text, anchor, at)
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
/// construction) cast `#: as !nil` stays Unknown (hover reports none),
/// and passing it to a sig expecting a concrete class is silent.
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
  kept = unknown_param #: as !nil
  InlineCastNilUnknownUser.new.take(
    kept, #: as !nil
  )
end
";
    assert_eq!(hover_in("nil_unknown.rb", src, "kept, #:", "kept"), None, "an Unknown param cast `!nil` must stay Unknown");
    let db = Db::default();
    let file = SourceFile::new(&db, "/p/nil_unknown.rb".into(), src.to_string());
    ProjectFiles::new(&db, vec![file]);
    let diags = check_file(&db, file);
    assert!(diags.is_empty(), "got: {diags:?}");
}

/// `!nil` on a real `String | nil` strips `Nil` to `String`, never coerces
/// to an unrelated class (hover reads it). Since MATRIX H2 the stripped
/// local is not judged by the `#:` sig; the literal beside it is.
#[test]
fn not_nil_on_unknown_param_never_narrows_away_a_real_mismatch() {
    let src = "\
class InlineCastNilRealSink
end

class InlineCastNilRealUser
  #: (InlineCastNilRealSink) -> void
  def take(target)
  end

  #: (?String? source) -> void
  def call_it(source = nil)
    stripped = source #: as !nil
    InlineCastNilRealUser.new.take(stripped)
    InlineCastNilRealUser.new.take(\"literal\")
  end
end
";
    assert_eq!(hover_in("nil_real.rb", src, "take(stripped", "stripped").as_deref(), Some("String"));
    let db = Db::default();
    let file = SourceFile::new(&db, "/p/nil_real.rb".into(), src.to_string());
    ProjectFiles::new(&db, vec![file]);
    let diags: Vec<String> = check_file(&db, file)
        .iter()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect();
    assert_eq!(diags, ["E0103 argument 1 of `take` expects InlineCastNilRealSink, got String"]);
}

/// `#: as Foo` retypes a genuinely mismatched argument to the real
/// project class it names — zero E0103.
#[test]
fn class_cast_makes_mismatched_arg_compatible() {
    let diags = check_fixture("class_cast_silent.rb");
    assert!(diags.is_empty(), "expected zero diagnostics, got: {diags:?}");
}

/// `#: as Blah` (no project class named `Blah`) must never invent a
/// type — the value keeps its real type, exactly as with no cast. The
/// cast local is silent (not a literal, MATRIX H2); the literal accuses.
#[test]
fn garbage_cast_stays_byte_identical_to_no_cast() {
    let name = "garbage_cast_still_accuses.rb";
    assert_eq!(hover_fixture(name, "kept, #:", "kept").as_deref(), Some("InlineCastWrongC"));
    assert_eq!(check_fixture(name), ["E0103 argument 1 of `use_foo` expects InlineCastFooC, got Symbol"]);
}

/// A cast that resolves fine to a REAL class retypes the value to it, but
/// does not silence the callee: the literal that does not match the
/// callee's declared param keeps accusing; the cast local is not judged.
#[test]
fn cast_to_real_class_still_wrong_for_callee_accuses() {
    let name = "still_invalid_after_cast_accuses.rb";
    assert_eq!(hover_fixture(name, "cast, #:", "cast").as_deref(), Some("InlineCastFooD"));
    assert_eq!(check_fixture(name), ["E0103 argument 1 of `use_other` expects InlineCastOtherD, got Symbol"]);
}

/// The cast comment binds to the ONE argument on its own physical line —
/// the uncast literal on the NEXT line of the same multi-line call is
/// still judged: exactly 1 E0103, for the second argument only. The call
/// passing locals is silent (MATRIX H2).
#[test]
fn multiline_call_second_arg_still_accuses() {
    let diags = check_fixture("multiline_no_leak_accuses.rb");
    assert_eq!(diags, ["E0103 argument 2 of `take_two` expects InlineCastFooE, got Symbol"]);
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
/// token) must still go through the existing `Named` arm — the value
/// keeps its real type (never erased). The literal accuses; the cast
/// local is not judged (MATRIX H2).
#[test]
fn untyped_lookalike_garbage_class_still_accuses() {
    let name = "untyped_lookalike_garbage_class_still_accuses.rb";
    assert_eq!(hover_fixture(name, "kept, #:", "kept").as_deref(), Some("InlineCastUntypedLookalikeWrong"));
    assert_eq!(
        check_fixture(name),
        ["E0103 argument 1 of `take` expects InlineCastUntypedLookalikeRight, got Symbol"]
    );
}

/// The `#: as untyped` cast binds to the ONE argument sharing its
/// physical line — the uncast literal on a different line of the same
/// multi-line call is still judged. Mutant (b): applying the cast to the
/// whole line/call instead of the one argument would silence it too.
#[test]
fn untyped_cast_no_leak_sibling_accuses() {
    let diags = check_fixture("untyped_cast_no_leak_sibling_accuses.rb");
    assert_eq!(diags, ["E0103 argument 2 of `take_two` expects InlineCastUntypedNoLeakFoo, got Symbol"]);
}

/// Control: a literal mismatch with NO cast comment at all must still
/// accuse — proves the new `Untyped` variant didn't widen silence beyond
/// lines that actually carry `#: as untyped`. The local beside it is not
/// judged (MATRIX H2).
#[test]
fn no_cast_real_mismatch_still_accuses() {
    let diags = check_fixture("no_cast_real_mismatch_still_accuses_e0z.rb");
    assert_eq!(diags, ["E0103 argument 1 of `take` expects InlineCastUntypedControlRight, got Symbol"]);
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
