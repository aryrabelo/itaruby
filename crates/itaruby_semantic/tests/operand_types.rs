//! E0108 — operator operand type mismatch on locally-provable values.
//!
//! Ground truth is MRI, not another checker's opinion (same discipline as
//! `scripts/inference-bench.jsonl`). Every accusing fixture below was
//! executed on `ruby 3.4.2 (2025-02-15) [arm64-darwin25]` and really
//! raises on the blamed line:
//!
//! ```text
//! price = 100; label = "R$ #{price}"; price + label
//!     TypeError: String can't be coerced into Integer
//! rate = 1.5; suffix = "x"; rate * suffix
//!     TypeError: String can't be coerced into Float
//! total = 100; discount = nil; total - discount
//!     TypeError: nil can't be coerced into Integer
//! label = "total: "; amount = 42; label + amount
//!     TypeError: no implicit conversion of Integer into String
//! 1 + "2"
//!     TypeError: String can't be coerced into Integer
//! ```
//!
//! and every silent fixture/control really runs clean, including the two
//! reopening controls whose whole point is that Ruby makes the pairing
//! LEGAL: `class String; def coerce(other) = [other, other]; end` makes
//! `100 + "R$ 100"` print `200`, and `class Integer; def +(other)` makes
//! it print `joined:R$`. A checker that accused either one would be
//! wrong about a running program, which is the false positive invariant
//! #1 forbids.
//!
//! Fixtures live in `testdata/operand_types/` (gate c checks the whole
//! tree as ONE merged project). The two reopening controls deliberately
//! do NOT live there: `core_class_unpolluted` is project-wide, so a
//! `class Integer` fragment anywhere in `testdata/` would silence every
//! accusing fixture in this family at once — the same reason
//! `testdata/core_conclusive/reopened_core_silent.rb` reopens `Array`
//! instead of `String`. They are proven here against isolated
//! single-file projects instead, each paired with the identical source
//! MINUS the reopening, so the silence is attributed to the reopening
//! and never to the harness.

use itaruby_semantic::{
    check_file, ClosedWorld, Db, LineIndex, ProjectFiles, Severity, SourceFile,
    E0108_OPERAND_TYPE_MISMATCH,
};

/// One diagnostic as `line:col:code: message`, 1-based exactly like the
/// CLI's own report — the column is in the key on purpose (AGENTS.md:
/// a diff key without the column cannot see two diagnostics on one line).
fn render(text: &str, diags: &[itaruby_semantic::Diagnostic]) -> Vec<String> {
    let index = LineIndex::new(text);
    diags
        .iter()
        .map(|d| {
            let (line, col) = index.line_col(text, d.start);
            format!(
                "{}:{}:{}[{}]: {}",
                line + 1,
                col + 1,
                if d.severity == Severity::Error {
                    "error"
                } else {
                    "warning"
                },
                d.code,
                d.message
            )
        })
        .collect()
}

/// A one-file project from source text, with closed world optionally on.
/// `ita check testdata/` runs closed-world (no Gemfile upward), so the
/// fixture helper below turns it on; E0108 itself never consults it,
/// which `fixture_verdict_is_identical_open_and_closed` proves.
fn check_source(name: &str, text: &str, closed: bool) -> Vec<String> {
    let db = Db::default();
    let file = SourceFile::new(&db, format!("/operand_types/{name}").into(), text.to_string());
    ProjectFiles::new(&db, vec![file]);
    if closed {
        ClosedWorld::new(&db, true);
    }
    render(text, check_file(&db, file))
}

fn fixture_text(name: &str) -> String {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/operand_types");
    std::fs::read_to_string(format!("{dir}/{name}")).unwrap()
}

fn check_fixture(name: &str) -> Vec<String> {
    let text = fixture_text(name);
    check_source(name, &text, true)
}

// -- accusing side: the pairings MRI raises TypeError on ------------------

/// The reference snippet, span and message pinned: the diagnostic must
/// land on the OPERAND (`label`, column 13 of line 10), the same
/// convention E0103 uses, and name both types.
#[test]
fn integer_plus_string_accuses_with_pinned_span_and_message() {
    assert_eq!(
        check_fixture("int_plus_string_accuses.rb"),
        vec!["10:13:error[E0108]: `+` on Integer expects a numeric operand, got String"],
    );
}

#[test]
fn float_times_string_accuses() {
    assert_eq!(
        check_fixture("float_times_string_accuses.rb"),
        vec!["8:12:error[E0108]: `*` on Float expects a numeric operand, got String"],
    );
}

#[test]
fn integer_minus_nil_accuses() {
    assert_eq!(
        check_fixture("int_minus_nil_accuses.rb"),
        vec!["9:13:error[E0108]: `-` on Integer expects a numeric operand, got nil"],
    );
}

#[test]
fn string_plus_integer_accuses() {
    assert_eq!(
        check_fixture("string_plus_int_accuses.rb"),
        vec!["9:13:error[E0108]: `+` on String expects a String operand, got Integer"],
    );
}

/// No locals at all: a literal proves itself, so the check never needs an
/// assignment to reach a verdict.
#[test]
fn two_literals_accuse() {
    assert_eq!(
        check_fixture("literal_pair_accuses.rb"),
        vec!["7:9:error[E0108]: `+` on Integer expects a numeric operand, got String"],
    );
}

/// The code constant is what the formatter and the LSP carry, so the
/// string is pinned once here rather than assumed by every assertion
/// above.
#[test]
fn the_emitted_code_is_the_exported_constant() {
    assert_eq!(E0108_OPERAND_TYPE_MISMATCH, "E0108");
    let diags = check_fixture("int_plus_string_accuses.rb");
    assert!(
        diags[0].contains(&format!("[{E0108_OPERAND_TYPE_MISMATCH}]")),
        "unexpected rendering: {diags:?}"
    );
}

/// E0108 does not consult `ClosedWorld` (unlike the core-method
/// conclusion in `check_call`): identical verdict either way, so the LSP
/// — which never enables closed world — reports it exactly like the CLI.
#[test]
fn fixture_verdict_is_identical_open_and_closed() {
    let text = fixture_text("int_plus_string_accuses.rb");
    assert_eq!(
        check_source("int_plus_string_accuses.rb", &text, false),
        check_source("int_plus_string_accuses.rb", &text, true),
    );
}

// -- silent side: fixtures that must stay quiet --------------------------

#[test]
fn silent_fixtures_report_nothing() {
    for name in [
        "reassigned_in_branch_silent.rb",
        "param_operand_silent.rb",
        "interpolation_only_silent.rb",
        "op_assign_silent.rb",
    ] {
        assert_eq!(check_fixture(name), Vec::<String>::new(), "fixture {name}");
    }
}

// -- the reopening controls, in isolated projects ------------------------

/// The exact source of the two controls below, minus the reopening: the
/// positive control that proves this harness DOES accuse, so each
/// silence below is attributable to the reopening alone and not to the
/// single-file project, the synthetic path, or the missing `class` body.
const BARE: &str = "\
price = 100
label = \"R$ #{price}\"
price + label
";

const REOPENED_INTEGER: &str = "\
class Integer
  def +(other)
    \"joined:#{other}\"
  end
end

price = 100
label = \"R$ #{price}\"
price + label
";

const STRING_WITH_COERCE: &str = "\
class String
  def coerce(other)
    [other, other]
  end
end

price = 100
label = \"R$ #{price}\"
price + label
";

#[test]
fn bare_source_accuses_so_the_controls_mean_something() {
    assert_eq!(
        check_source("bare.rb", BARE, true),
        vec!["3:9:error[E0108]: `+` on Integer expects a numeric operand, got String"],
    );
}

/// `class Integer; def +(other)` — MRI prints `joined:R$`, so accusing
/// would be a false positive on a program that runs. The project
/// reopened the receiver's core class, so `core_class_unpolluted` is
/// false and the site stays silent.
#[test]
fn reopened_integer_with_custom_plus_is_silent() {
    assert_eq!(
        check_source("reopened_integer.rb", REOPENED_INTEGER, true),
        Vec::<String>::new(),
    );
}

/// `class String; def coerce` — MRI prints `200`. The reopening is on the
/// ARGUMENT's class, not the receiver's, which is why
/// `check_operand_types` gates on both sides: gating on the receiver
/// alone would accuse this running program.
#[test]
fn string_with_coerce_is_silent() {
    assert_eq!(
        check_source("string_coerce.rb", STRING_WITH_COERCE, true),
        Vec::<String>::new(),
    );
}

// -- boundaries of the flow-insensitive proof ---------------------------

/// Two writes of the SAME literal type still prove the type — the rule is
/// "every write agrees", not "at most one write". Without this, the
/// mutant that poisons on any second write would pass unnoticed.
#[test]
fn two_writes_of_the_same_type_still_accuse() {
    let text = "\
price = 100
price = 200
label = \"R$\"
price + label
";
    assert_eq!(
        check_source("two_writes.rb", text, true),
        vec!["4:9:error[E0108]: `+` on Integer expects a numeric operand, got String"],
    );
}

/// A reassignment BELOW the operator silences the operator too: the
/// flow-insensitive half reads the whole scope, so this site is
/// unprovable even though the flow-sensitive env alone would call it an
/// Integer. Fail-closed, and the direction the flow-sensitive walk cannot
/// see by itself.
#[test]
fn a_later_reassignment_silences_an_earlier_site() {
    let text = "\
price = 100
label = \"R$\"
price + label
price = \"free\"
";
    assert_eq!(
        check_source("later_write.rb", text, true),
        Vec::<String>::new(),
    );
}

/// The env half of `proven_operand`, on the ARGUMENT side — the only
/// side where it is load-bearing (the receiver is independently pinned
/// by `check_operand_types`'s `*recv_ty != lhs` guard). `label` has one
/// single literal write, so the flow-insensitive scan proves `String`,
/// but the walker's env at the operator does not agree: on the skipped
/// branch the name is `nil`. MRI does raise here — with `nil can't be
/// coerced into Integer`, a DIFFERENT message than this site would print
/// — so silence is the honest verdict, and this control is what makes
/// dropping the env comparison fail the suite.
#[test]
fn a_conditionally_assigned_operand_is_not_proven() {
    let text = "\
flag = nil
label = \"R$\" if flag
price = 100
price + label
";
    assert_eq!(
        check_source("conditional_arg.rb", text, true),
        Vec::<String>::new(),
    );
}

/// Same shape with the conditional on the RECEIVER: silent for the same
/// reason, through the `*recv_ty != lhs` guard rather than through
/// `proven_operand`'s env comparison.
#[test]
fn a_conditionally_assigned_receiver_is_not_proven() {
    let text = "\
flag = nil
price = 100 if flag
label = \"R$\"
price + label
";
    assert_eq!(
        check_source("conditional_recv.rb", text, true),
        Vec::<String>::new(),
    );
}

/// A write from an unreadable right-hand side (a method call) poisons the
/// name: `Integer("3")` is a `String`-free Integer at runtime, but this
/// check never guesses at a call's result.
#[test]
fn a_call_fed_local_is_never_proven() {
    let text = "\
price = Integer(\"3\")
label = \"R$\"
price + label
";
    assert_eq!(
        check_source("call_fed.rb", text, true),
        Vec::<String>::new(),
    );
}

/// `eval` can rewrite any local in the scope with content no AST scan can
/// read, so one `eval` call drops the whole scope's proof.
#[test]
fn an_eval_in_the_scope_drops_every_proof() {
    let text = "\
price = 100
label = \"R$\"
eval(\"price = 'free'\")
price + label
";
    assert_eq!(check_source("evaled.rb", text, true), Vec::<String>::new());
}

/// Ruby's scope gates: the `price` written in the method body is NOT the
/// toplevel `price`, so the toplevel write must not prove the body's
/// read (which is a plain `nil`-valued local there, not an Integer).
/// Nothing fires — the body's own `price` has no literal write at all.
#[test]
fn a_def_opens_a_fresh_local_scope() {
    let text = "\
price = 100

def OperandTypesScope_run(price)
  label = \"R$\"
  price + label
end
";
    assert_eq!(check_source("scope.rb", text, true), Vec::<String>::new());
}

/// Legal Ruby that shares every ingredient with the accusing shapes:
/// mixed numerics, `String#*` with an Integer, `String#+` with a String.
/// This is the silence side of the two-sided proof at the pairing table.
#[test]
fn legal_pairings_stay_silent() {
    let text = "\
whole = 100
part = 1.5
label = \"ab\"
factor = 2
suffix = \"c\"
whole + part
label * factor
label + suffix
";
    assert_eq!(check_source("legal.rb", text, true), Vec::<String>::new());
}

// -- refinements (round-3 critic, invariant #1 violation) ----------------
//
// `refine Integer do def +(o) ... end end` creates no fragment and no
// injection call, so before the `refine` arm in `index.rs` these two
// programs — which MRI runs clean — were accused. Measured on
// ruby 3.4.2: the Integer source prints `"refined s"` and the String
// source prints `"refined 2"`, both exit 0.
//
// Like the two reopening controls above, these sources are isolated
// single-file projects instead of `testdata/` fixtures, and for the same
// measured reason: `refined_core` is project-wide, so dropping a
// `refine Integer` file into `testdata/operand_types/` took
// `ita check testdata/` from 5 E0108 diagnostics to 1 (and silenced an
// E0101 too). The `mri_ground_truth_is_executed` test below writes them
// out as `refined_integer_plus_silent.rb` / `refined_string_plus_silent.rb`
// and really runs them.

const REFINED_INTEGER_PLUS: &str = "\
module OperandTypesIntPlus
  refine Integer do
    def +(other) = \"refined #{other}\"
  end
end

using OperandTypesIntPlus

price = 1
label = \"s\"
price + label
";

const REFINED_STRING_PLUS: &str = "\
module OperandTypesStrPlus
  refine String do
    def +(other) = \"refined #{other}\"
  end
end

using OperandTypesStrPlus

label = \"ab\"
amount = 2
label + amount
";

/// The control that keeps the arm from being a blanket switch: a
/// refinement of an UNRELATED core class leaves `Integer` provable, so
/// the site still accuses. Without this, marking `core_mixin` for any
/// `refine` would pass every other test here.
const REFINED_ARRAY_UNRELATED: &str = "\
module OperandTypesArrayRefine
  refine Array do
    def second = self[1]
  end
end

using OperandTypesArrayRefine

price = 100
label = \"R$\"
price + label
";

#[test]
fn a_refined_integer_plus_is_silent() {
    assert_eq!(
        check_source("refined_integer_plus_silent.rb", REFINED_INTEGER_PLUS, true),
        Vec::<String>::new(),
    );
}

#[test]
fn a_refined_string_plus_is_silent() {
    assert_eq!(
        check_source("refined_string_plus_silent.rb", REFINED_STRING_PLUS, true),
        Vec::<String>::new(),
    );
}

#[test]
fn refining_an_unrelated_core_class_still_accuses() {
    assert_eq!(
        check_source("refined_array.rb", REFINED_ARRAY_UNRELATED, true),
        vec!["11:9:error[E0108]: `+` on Integer expects a numeric operand, got String"],
    );
}

/// Every target shape prism produces for a refinement resolves to the
/// same name: bare, `::`-rooted (cbase), parenthesized, and qualified.
/// Since round 5 the BODY decides as well as the target, so each form is
/// measured twice: with `def +` it silences its own class, with an empty
/// block it does not — an empty refinement changes no operator, and MRI
/// keeps raising (`refined_array_unrelated.rb`'s transcript is the same
/// shape).
///
/// The block-ARGUMENT form (`refine Integer, &blk`) is the one exception
/// and stays silent in both columns: the body is not in this file, so it
/// is `Opaque`. Measured on ruby 3.4.2 it raises `ArgumentError: can't
/// pass a Proc as a block to Module#refine` before any operand runs, so
/// standing down on it costs a false negative only on code that already
/// crashes.
#[test]
fn every_refine_target_form_is_read_by_name() {
    let tail = "\nprice = 100\nlabel = \"R$\"\nprice + label\n";
    for (form, silent) in [
        ("module M\n  refine Integer do\n    def +(o) = 1\n  end\nend\nusing M\n", true),
        ("module M\n  refine ::Integer do\n    def +(o) = 1\n  end\nend\nusing M\n", true),
        ("module M\n  refine(Integer) do\n    def +(o) = 1\n  end\nend\nusing M\n", true),
        ("module M\n  blk = proc {}\n  refine Integer, &blk\nend\nusing M\n", true),
        ("module M\n  refine Integer do\n  end\nend\nusing M\n", false),
        ("module M\n  refine ::Integer do\n  end\nend\nusing M\n", false),
        ("module M\n  refine(Integer) do\n  end\nend\nusing M\n", false),
    ] {
        let text = format!("{form}{tail}");
        let got = check_source("refine_form.rb", &text, true);
        assert_eq!(got.is_empty(), silent, "form: {form} got: {got:?}");
    }
}

/// Fail-closed decision recorded in the arm, now keyed by NAME: a refine
/// target that cannot be named (`refine klass do`) means "some core
/// class was refined, unknown which", so whatever the body defines
/// counts against EVERY core class — and only what it defines. An empty
/// body changes no operator anywhere, which is why the first column
/// accuses; a body that defines `+` could have replaced any core class's
/// operator, which is why the second is silent.
#[test]
fn an_unnamable_refine_target_is_read_by_name() {
    let tail = "\nprice = 100\nlabel = \"R$\"\nprice + label\n";
    for (body, silent) in [("", false), ("    def +(o) = 1\n", true)] {
        let text = format!("klass = Integer\nmodule M\n  refine klass do\n{body}  end\nend\n{tail}");
        let got = check_source("refine_dynamic.rb", &text, true);
        assert_eq!(got.is_empty(), silent, "body: {body:?} got: {got:?}");
    }
}

/// The other side of that decision: a refinement whose target IS named
/// and is not a core class poisons nothing. `refine` is stored as
/// written minus a leading `::`, so a qualified project constant can
/// never collide with a core name.
#[test]
fn refining_a_project_constant_poisons_nothing() {
    let text = "\
module OperandTypesProj
  class Integer
  end

  refine OperandTypesProj::Integer do
  end
end

price = 100
label = \"R$\"
price + label
";
    assert_eq!(
        check_source("refine_project_const.rb", text, true),
        vec!["11:9:error[E0108]: `+` on Integer expects a numeric operand, got String"],
    );
}

// -- MRI ground truth, executed ------------------------------------------

/// What MRI really does, run rather than quoted (round-3 critic: the
/// `*_accuses.rb` bodies were never invoked, so the header's transcript
/// was a claim about a program nobody had executed in CI). Every fixture
/// in `testdata/operand_types/` now ends in a top-level driver, so the
/// bytes the checker reads are the bytes `ruby` runs — no rewriting, no
/// generated copy.
///
/// Three outcomes, and the difference between them is the point:
/// `Raises` is a bug itaruby reports, `Clean` is code itaruby must stay
/// quiet on, and `Rescued(n)` is code that really raises where itaruby is
/// deliberately silent — the accepted false negatives of the
/// binding-form and parameter rules, counted instead of assumed.
enum Mri {
    /// Raises `TypeError`, and the backtrace names this 1-based line —
    /// the same line the E0108 assertions above pin.
    Raises(u32),
    /// Exits 0 with nothing raised.
    Clean,
    /// Exits 0 after its driver rescued exactly this many `TypeError`s.
    Rescued(usize),
}

fn ruby_available() -> bool {
    std::process::Command::new("ruby")
        .arg("-v")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Runs `ruby <path>` and returns `(exit code, stdout+stderr)`.
fn run_ruby(path: &std::path::Path) -> (i32, String) {
    let out = std::process::Command::new("ruby")
        .arg(path)
        .output()
        .expect("ruby is on PATH (checked by the caller)");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

fn assert_mri(name: &str, path: &std::path::Path, expected: &Mri) {
    let (code, text) = run_ruby(path);
    match expected {
        Mri::Raises(line) => {
            assert_eq!(code, 1, "{name} must fail: {text}");
            assert!(text.contains("TypeError"), "{name} must raise TypeError: {text}");
            let blamed = format!(":{line}:in ");
            assert!(
                text.contains(&blamed),
                "{name} must raise on line {line}: {text}"
            );
        }
        Mri::Clean => {
            assert_eq!(code, 0, "{name} must run clean: {text}");
            assert!(!text.contains("TypeError"), "{name} raised: {text}");
        }
        Mri::Rescued(n) => {
            assert_eq!(code, 0, "{name} must exit 0: {text}");
            assert_eq!(
                text.matches("TypeError").count(),
                *n,
                "{name} must rescue {n} TypeError(s): {text}"
            );
        }
    }
}

#[test]
fn mri_ground_truth_is_executed() {
    if !ruby_available() {
        eprintln!("skipping: no `ruby` on PATH — the MRI leg is machine-dependent");
        return;
    }
    let dir = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/operand_types"
    ));
    // The blamed lines are the SAME numbers the E0108 assertions pin
    // above, which is what makes this a check on the diagnostic and not
    // just on Ruby.
    let table = [
        ("int_plus_string_accuses.rb", Mri::Raises(10)),
        ("float_times_string_accuses.rb", Mri::Raises(8)),
        ("int_minus_nil_accuses.rb", Mri::Raises(9)),
        ("string_plus_int_accuses.rb", Mri::Raises(9)),
        ("literal_pair_accuses.rb", Mri::Raises(7)),
        ("interpolation_only_silent.rb", Mri::Clean),
        ("op_assign_silent.rb", Mri::Rescued(3)),
        ("param_operand_silent.rb", Mri::Rescued(1)),
        ("reassigned_in_branch_silent.rb", Mri::Rescued(1)),
    ];
    for (name, expected) in &table {
        assert_mri(name, &dir.join(name), expected);
    }
    // Every fixture in the directory is in the table: a new fixture must
    // declare what MRI does with it instead of arriving unmeasured.
    let mut on_disk: Vec<String> = std::fs::read_dir(dir)
        .expect("fixture dir")
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "rb"))
        .map(|p| p.file_name().expect("file name").to_string_lossy().into_owned())
        .collect();
    on_disk.sort();
    let mut declared: Vec<String> = table.iter().map(|(n, _)| (*n).to_string()).collect();
    declared.sort();
    assert_eq!(on_disk, declared, "every fixture must declare its MRI outcome");
}

// -- refinements at any nesting, and through aliases (round-4 critic) ----
//
// Three more shapes MRI runs clean that the body-walker arm still
// accused, all measured on ruby 3.4.2 against the review's battery (a
// local scratch copy, not versioned): a target reached through a
// constant ALIAS,
// a `refine` executed from a METHOD body, and one inside a
// `Module.new do ... end` BLOCK. The arm moved to `RefineScan`, a
// file-level prism `Visit` that sees every nesting, and the target is
// chased through constant aliases after the merge
// (`resolve_refined_core`).

const REFINED_VIA_ALIAS: &str = "\
I = Integer

module OperandTypesAliasRef
  refine I do
    def +(other) = \"aliased #{other}\"
  end
end

using OperandTypesAliasRef

price = 1
label = \"s\"
price + label
";

const REFINED_IN_METHOD: &str = "\
module OperandTypesMethRef
  def self.install
    refine Integer do
      def +(other) = \"meth #{other}\"
    end
  end
  install
end

using OperandTypesMethRef

price = 1
label = \"s\"
price + label
";

const REFINED_IN_MODULE_NEW: &str = "\
OperandTypesAnonRef = Module.new do
  refine Integer do
    def +(other) = \"anon #{other}\"
  end
end

using OperandTypesAnonRef

price = 1
label = \"s\"
price + label
";

/// The control that keeps alias resolution from degenerating into
/// "anything aliased poisons everything": `A = Array` refines Array, so
/// `Integer + String` must still accuse. Without it, resolving the alias
/// to a blanket mark would pass every other test in this section.
const REFINED_ARRAY_VIA_ALIAS: &str = "\
A = Array

module OperandTypesArrayAliasRef
  refine A do
    def second = self[1]
  end
end

using OperandTypesArrayAliasRef

price = 100
label = \"R$\"
price + label
";

#[test]
fn a_refinement_through_a_constant_alias_is_silent() {
    assert_eq!(
        check_source("refined_via_alias_silent.rb", REFINED_VIA_ALIAS, true),
        Vec::<String>::new(),
    );
}

#[test]
fn a_refinement_inside_a_method_body_is_silent() {
    assert_eq!(
        check_source("refined_in_method_silent.rb", REFINED_IN_METHOD, true),
        Vec::<String>::new(),
    );
}

#[test]
fn a_refinement_inside_a_module_new_block_is_silent() {
    assert_eq!(
        check_source("refined_in_module_new_silent.rb", REFINED_IN_MODULE_NEW, true),
        Vec::<String>::new(),
    );
}

#[test]
fn refining_an_unrelated_core_class_through_an_alias_still_accuses() {
    assert_eq!(
        check_source("refined_array_alias.rb", REFINED_ARRAY_VIA_ALIAS, true),
        vec!["13:9:error[E0108]: `+` on Integer expects a numeric operand, got String"],
    );
}

/// A refinement of a PROJECT class still poisons nothing, alias or not —
/// the round-3 decision, re-proven now that the target goes through
/// `resolve_refined_core` (`refine Foo` where `Foo` is a project class,
/// the review's `r_project_only.rb`: MRI really raises on the operator
/// line, so accusing is correct).
#[test]
fn refining_a_project_class_still_accuses() {
    let text = "\
class OperandTypesFoo
end

module OperandTypesFooRef
  refine OperandTypesFoo do
    def bar = 1
  end
end

using OperandTypesFooRef

price = 100
label = \"R$\"
price + label
";
    assert_eq!(
        check_source("refine_project_class.rb", text, true),
        vec!["14:9:error[E0108]: `+` on Integer expects a numeric operand, got String"],
    );
}

/// The sources that cannot live in `testdata/` (see the refinement
/// section's comment) get the same executed treatment, written out under
/// the names the round-3 critic asked for. The two reopening controls
/// ride along: their "MRI prints 200 / joined:R$" claim was prose until
/// now.
#[test]
fn mri_ground_truth_for_the_isolated_sources() {
    if !ruby_available() {
        eprintln!("skipping: no `ruby` on PATH — the MRI leg is machine-dependent");
        return;
    }
    // Keyed by pid, not by test name: `CARGO_TARGET_TMPDIR` is shared
    // across every run against this target dir, and a second agent
    // running this same suite deleted the directory mid-test — the
    // identical defect AGENTS.md records for `json_format.rs`, observed
    // four times while this round was being written (a red that always
    // passed on a pinned re-run).
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("operand_types_isolated_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create tmpdir");
    for (name, src, expected) in [
        ("refined_integer_plus_silent.rb", REFINED_INTEGER_PLUS, Mri::Clean),
        ("refined_string_plus_silent.rb", REFINED_STRING_PLUS, Mri::Clean),
        ("refined_array_unrelated.rb", REFINED_ARRAY_UNRELATED, Mri::Raises(11)),
        ("refined_via_alias_silent.rb", REFINED_VIA_ALIAS, Mri::Clean),
        ("refined_in_method_silent.rb", REFINED_IN_METHOD, Mri::Clean),
        ("refined_in_module_new_silent.rb", REFINED_IN_MODULE_NEW, Mri::Clean),
        ("refined_array_alias.rb", REFINED_ARRAY_VIA_ALIAS, Mri::Raises(13)),
        ("bare.rb", BARE, Mri::Raises(3)),
        ("reopened_integer.rb", REOPENED_INTEGER, Mri::Clean),
        ("string_coerce.rb", STRING_WITH_COERCE, Mri::Clean),
        ("eval_string_in_method_silent.rb", EVAL_STRING_IN_METHOD, Mri::Clean),
        ("eval_heredoc_in_method_silent.rb", EVAL_HEREDOC_IN_METHOD, Mri::Clean),
        ("module_eval_string_in_method_silent.rb", MODULE_EVAL_IN_METHOD, Mri::Clean),
        (
            "instance_eval_define_method_silent.rb",
            INSTANCE_EVAL_DEFINE_METHOD,
            Mri::Clean,
        ),
        ("eval_via_alias_silent.rb", EVAL_VIA_ALIAS, Mri::Clean),
        ("eval_dynamic_receiver_silent.rb", EVAL_DYNAMIC_RECEIVER, Mri::Clean),
        ("eval_const_get_receiver_silent.rb", EVAL_CONST_GET_RECEIVER, Mri::Clean),
        ("bare_eval_silent.rb", BARE_EVAL, Mri::Clean),
        ("kernel_eval_silent.rb", KERNEL_EVAL, Mri::Clean),
        ("binding_eval_silent.rb", BINDING_EVAL, Mri::Clean),
        ("class_eval_block_in_method_silent.rb", EVAL_BLOCK_IN_METHOD, Mri::Clean),
        ("class_eval_block_unrelated.rb", EVAL_BLOCK_UNRELATED, Mri::Raises(10)),
        ("eval_array_in_method.rb", EVAL_ARRAY_IN_METHOD, Mri::Raises(8)),
        ("eval_project_class_in_method.rb", EVAL_PROJECT_CLASS, Mri::Raises(11)),
        (
            "receiverless_eval_in_project_class.rb",
            EVAL_RECEIVERLESS_PROJECT,
            Mri::Raises(5),
        ),
        ("no_eval_control.rb", EVAL_NONE_CONTROL, Mri::Raises(8)),
        ("keyed_reopen_unrelated.rb", KEYED_REOPEN_UNRELATED, Mri::Raises(7)),
        ("keyed_reopen_plus_silent.rb", KEYED_REOPEN_PLUS, Mri::Clean),
        ("keyed_object_plus_still_accuses.rb", KEYED_OBJECT_PLUS, Mri::Raises(7)),
        ("keyed_object_coerce_silent.rb", KEYED_OBJECT_COERCE, Mri::Clean),
        ("keyed_to_str_hook_silent.rb", KEYED_TO_STR_HOOK, Mri::Clean),
        ("keyed_attr_accessor_accuses.rb", KEYED_ATTR_ACCESSOR, Mri::Raises(7)),
        ("keyed_block_attr_accuses.rb", KEYED_BLOCK_ATTR, Mri::Raises(5)),
        ("keyed_block_arg_unnamable_accuses.rb", KEYED_BLOCK_ARG_UNNAMABLE, Mri::Raises(5)),
        ("keyed_include_plus.rb", KEYED_INCLUDE_PLUS, Mri::Rescued(1)),
        ("keyed_include_unrelated_accuses.rb", KEYED_INCLUDE_UNRELATED, Mri::Raises(9)),
        ("keyed_reopen_include_silent.rb", KEYED_REOPEN_INCLUDE, Mri::Clean),
        ("keyed_dynamic_mixin_arg_silent.rb", KEYED_DYNAMIC_MIXIN_ARG, Mri::Clean),
        ("keyed_unknown_macro_fail_closed.rb", KEYED_UNKNOWN_MACRO, Mri::Rescued(1)),
        ("keyed_dynamic_define_method.rb", KEYED_DYNAMIC_DEFINE_METHOD, Mri::Rescued(1)),
        ("keyed_refine_unrelated_accuses.rb", KEYED_REFINE_UNRELATED, Mri::Raises(11)),
        ("keyed_refine_unknown_macro.rb", KEYED_REFINE_MACRO, Mri::Rescued(1)),
        ("keyed_method_missing_on_arg_silent.rb", KEYED_MM_ON_ARG, Mri::Clean),
        ("keyed_method_missing_on_receiver.rb", KEYED_MM_ON_RECEIVER, Mri::Raises(7)),
        ("keyed_alias_reopening_silent.rb", KEYED_ALIAS_REOPENING, Mri::Clean),
        ("keyed_block_include_still_accuses.rb", KEYED_BLOCK_INCLUDE, Mri::Raises(15)),
        ("keyed_toplevel_method_missing.rb", KEYED_TOPLEVEL_MM, Mri::Rescued(1)),
    ] {
        let path = dir.join(name);
        // A bare expression result is not printed by `ruby`, so the
        // clean sources need no driver: "exit 0, nothing raised" is the
        // whole claim, and the accusing ones raise on their own.
        std::fs::write(&path, src).expect("write source");
        assert_mri(name, &path, &expected);
    }
}

// -- eval bodies nobody showed the checker -------------------------------
//
// `Integer.class_eval("def +(o) = 'x'")` replaces a core operator through
// a body no AST can read. The literal-receiver shape at toplevel or in a
// class body was already covered by `core_injection_call` (project-wide
// `core_mixin`); every source below was measured a LIVE false positive
// against ruby 3.4.2 before `FileScan::note_opaque_eval` existed — `ita
// check` accusing the operator line, `ruby` exiting 0 and printing the
// evaled result. All of them are executed by
// `mri_ground_truth_for_the_isolated_sources`, in the same table as the
// refinement sources, and they are isolated single-file projects for the
// same measured reason the refinement sources are: pollution is
// project-wide, so one of these files inside `testdata/operand_types/`
// would silence every accusing fixture in the directory.
//
// The operands are LITERALS on purpose. With locals
// (`price = 1; label = "s"`) an eval in the SAME Ruby scope already
// silences the site through `OperandLocalScan`'s bail, so a local-operand
// fixture would prove the locals map rather than the pollution mark —
// measured: the dynamic-receiver, `Kernel.eval` and `binding.eval`
// sources were silent before this change when written with locals, and
// accusing when written with literals.

const EVAL_STRING_IN_METHOD: &str = "\
module OperandTypesEvalMeth
  def self.install
    Integer.class_eval(\"def +(other) = 'evaled'\")
  end
  install
end

p 1 + \"s\"
";

const EVAL_HEREDOC_IN_METHOD: &str = "\
module OperandTypesEvalHeredoc
  def self.install
    Integer.class_eval(<<~RUBY)
      def +(other) = 'heredoc'
    RUBY
  end
  install
end

p 1 + \"s\"
";

const MODULE_EVAL_IN_METHOD: &str = "\
module OperandTypesModuleEval
  def self.install
    Integer.module_eval(\"def +(other) = 'module_evaled'\")
  end
  install
end

p 1 + \"s\"
";

/// `instance_eval` is in the collector's list because the "it only
/// touches the singleton" reading is false: `define_method` inside it
/// runs with `self == Integer`, so it defines an INSTANCE method. This
/// source really prints `"instance_evaled"` (measured).
const INSTANCE_EVAL_DEFINE_METHOD: &str = "\
module OperandTypesInstanceEval
  def self.install
    Integer.instance_eval(\"define_method(:+) { |other| 'instance_evaled' }\")
  end
  install
end

p 1 + \"s\"
";

const EVAL_VIA_ALIAS: &str = "\
OperandTypesEvalAlias = Integer

module OperandTypesEvalAliasHost
  def self.install
    OperandTypesEvalAlias.class_eval(\"def +(other) = 'aliased'\")
  end
  install
end

p 1 + \"s\"
";

const EVAL_DYNAMIC_RECEIVER: &str = "\
klass = Integer
klass.class_eval(\"def +(other) = 'dynamic'\")

p 1 + \"s\"
";

const EVAL_CONST_GET_RECEIVER: &str = "\
Object.const_get(\"Integer\").class_eval(\"def +(other) = 'const_get'\")

p 1 + \"s\"
";

const BARE_EVAL: &str = "\
eval(\"class Integer; def +(other) = 'bare_eval'; end\")

p 1 + \"s\"
";

const KERNEL_EVAL: &str = "\
Kernel.eval(\"class Integer; def +(other) = 'kernel_eval'; end\")

p 1 + \"s\"
";

const BINDING_EVAL: &str = "\
binding.eval(\"class Integer; def +(other) = 'binding_eval'; end\")

p 1 + \"s\"
";

const EVAL_BLOCK_IN_METHOD: &str = "\
module OperandTypesEvalBlock
  def self.install
    Integer.class_eval do
      def +(other) = 'block'
    end
  end
  install
end

p 1 + \"s\"
";

/// The measured COST of treating a block body as pollution: this program
/// really raises (MRI, line 10) and itaruby is now silent on it. Kept as
/// a test so the false negative is counted rather than discovered later —
/// and it is the same false negative the walker-reachable contour has
/// always had, where `Integer.class_eval { def doubled = self * 2 }` at
/// toplevel already sets `core_mixin`.
const EVAL_BLOCK_UNRELATED: &str = "\
module OperandTypesEvalBlockUnrelated
  def self.install
    Integer.class_eval do
      def doubled = self * 2
    end
  end
  install
end

p 1 + \"s\"
";

/// The control that keeps the mark from being a blanket switch: evaling
/// into `Array` leaves `Integer` provable, so the site still accuses —
/// and MRI really raises there.
const EVAL_ARRAY_IN_METHOD: &str = "\
module OperandTypesEvalArray
  def self.install
    Array.class_eval(\"def second = self[1]\")
  end
  install
end

p 1 + \"s\"
";

const EVAL_PROJECT_CLASS: &str = "\
class OperandTypesEvalProj
end

module OperandTypesEvalProjHost
  def self.install
    OperandTypesEvalProj.class_eval(\"def zz = 1\")
  end
  install
end

p 1 + \"s\"
";

/// A receiverless `class_eval` resolves to the enclosing class (the Rails
/// `class_eval <<~RUBY` idiom), never to "unknown" — otherwise the idiom
/// would stand every core class down for free. Here the enclosing class
/// is a project class, so `1 + \"s\"` must still accuse.
const EVAL_RECEIVERLESS_PROJECT: &str = "\
class OperandTypesEvalReceiverless
  class_eval(\"def zz = 1\")
end

p 1 + \"s\"
";

/// The harness's own positive control: the identical shape MINUS the
/// eval, so every silence above is attributed to the eval and never to
/// the fixture (AGENTS.md: a silence row needs a positive control or it
/// proves nothing).
const EVAL_NONE_CONTROL: &str = "\
module OperandTypesEvalNone
  def self.install
    1
  end
  install
end

p 1 + \"s\"
";

#[test]
fn a_string_class_eval_from_a_method_body_is_silent() {
    assert_eq!(
        check_source("eval_string_in_method_silent.rb", EVAL_STRING_IN_METHOD, true),
        Vec::<String>::new(),
    );
}

/// Every string-bodied form of the same call silences its receiver: a
/// heredoc argument, `module_eval`, and `instance_eval` (whose
/// `define_method` body really lands on the instance track).
#[test]
fn every_string_eval_form_silences_its_receiver() {
    for (name, src) in [
        ("eval_heredoc_in_method_silent.rb", EVAL_HEREDOC_IN_METHOD),
        ("module_eval_string_in_method_silent.rb", MODULE_EVAL_IN_METHOD),
        ("instance_eval_define_method_silent.rb", INSTANCE_EVAL_DEFINE_METHOD),
    ] {
        assert_eq!(check_source(name, src, true), Vec::<String>::new(), "source: {name}");
    }
}

#[test]
fn an_eval_target_through_a_constant_alias_is_silent() {
    assert_eq!(
        check_source("eval_via_alias_silent.rb", EVAL_VIA_ALIAS, true),
        Vec::<String>::new(),
    );
}

/// Fail-closed, recorded in `note_opaque_eval`: a string eval whose
/// receiver cannot be named means "some class got a body we cannot read,
/// unknown which", which stands EVERY core class down rather than none.
#[test]
fn an_unnamable_eval_receiver_silences_every_core_class() {
    for (name, src) in [
        ("eval_dynamic_receiver_silent.rb", EVAL_DYNAMIC_RECEIVER),
        ("eval_const_get_receiver_silent.rb", EVAL_CONST_GET_RECEIVER),
    ] {
        assert_eq!(check_source(name, src, true), Vec::<String>::new(), "source: {name}");
    }
}

/// A bare `eval` names no receiver at all and its string can define
/// anything anywhere, so it stands every core class down — same decision,
/// reached through `eval`, `Kernel.eval` and `binding.eval`.
#[test]
fn a_bare_eval_silences_every_core_class() {
    for (name, src) in [
        ("bare_eval_silent.rb", BARE_EVAL),
        ("kernel_eval_silent.rb", KERNEL_EVAL),
        ("binding_eval_silent.rb", BINDING_EVAL),
    ] {
        assert_eq!(check_source(name, src, true), Vec::<String>::new(), "source: {name}");
    }
}

#[test]
fn a_block_class_eval_from_a_method_body_is_silent() {
    assert_eq!(
        check_source("class_eval_block_in_method_silent.rb", EVAL_BLOCK_IN_METHOD, true),
        Vec::<String>::new(),
    );
}

/// Round 4 recorded this as an accepted false negative: a block
/// `class_eval` body that defines `doubled` and nothing else stood
/// `Integer` down anyway. Name-keying turns it into a true positive —
/// MRI raises on line 10 (`class_eval_block_unrelated.rb` in the ground
/// truth table) and the site now says so.
#[test]
fn a_block_eval_body_that_defines_nothing_relevant_still_accuses() {
    assert_eq!(
        check_source("class_eval_block_unrelated.rb", EVAL_BLOCK_UNRELATED, true),
        vec!["10:7:error[E0108]: `+` on Integer expects a numeric operand, got String"],
    );
}

#[test]
fn an_eval_on_an_unrelated_core_class_still_accuses() {
    assert_eq!(
        check_source("eval_array_in_method.rb", EVAL_ARRAY_IN_METHOD, true),
        vec!["8:7:error[E0108]: `+` on Integer expects a numeric operand, got String"],
    );
}

#[test]
fn an_eval_on_a_project_class_poisons_nothing() {
    assert_eq!(
        check_source("eval_project_class_in_method.rb", EVAL_PROJECT_CLASS, true),
        vec!["11:7:error[E0108]: `+` on Integer expects a numeric operand, got String"],
    );
}

#[test]
fn a_receiverless_eval_in_a_project_class_still_accuses() {
    assert_eq!(
        check_source(
            "receiverless_eval_in_project_class.rb",
            EVAL_RECEIVERLESS_PROJECT,
            true
        ),
        vec!["5:7:error[E0108]: `+` on Integer expects a numeric operand, got String"],
    );
}

#[test]
fn the_same_source_without_any_eval_accuses() {
    assert_eq!(
        check_source("no_eval_control.rb", EVAL_NONE_CONTROL, true),
        vec!["8:7:error[E0108]: `+` on Integer expects a numeric operand, got String"],
    );
}

// -- name-keyed pollution: does the source define the OPERATOR? ----------
//
// The blanket question ("was this class touched at all?") stood E0108
// down on every core-class reopening in every real project; the keyed
// question ("could this source have defined `+`, or the conversion hook
// the operator consults?") is a different question with a measured
// different answer — 248 methods are defined directly inside core
// reopenings across rails/mastodon/discourse and ZERO of them is an
// operator or a coercion hook (AGENTS.md). Every source below is a
// single-file project for the same reason the refinement sources are:
// pollution is project-wide, so one of these inside
// `testdata/operand_types/` would silence every accusing neighbor.
//
// Each pair is two-sided on purpose, and the asymmetry between the two
// sides is MRI's, not a shortcut: the RECEIVER's operator can only be
// replaced on its own class (`Object#+` still raises), while the
// ARGUMENT's conversion hook is found anywhere in its ancestry
// (`Object#coerce` makes `1 + "s"` print `2`). Both transcripts are in
// `core.rs`'s `core_own_names`/`arg_pollution_keys`, and every source
// here is executed by `mri_ground_truth_for_the_isolated_sources`.
const KEYED_REOPEN_UNRELATED: &str = "\
class Integer
  def zz = 1
end

price = 100
label = \"R$\"
p price + label
";
const KEYED_REOPEN_PLUS: &str = "\
class Integer
  def +(other) = \"keyed\"
end

price = 100
label = \"R$\"
p price + label
";
const KEYED_OBJECT_PLUS: &str = "\
class Object
  def +(other) = \"object\"
end

price = 100
label = \"R$\"
p price + label
";
const KEYED_OBJECT_COERCE: &str = "\
class Object
  def coerce(other) = [1, 1]
end

price = 100
label = \"R$\"
p price + label
";
const KEYED_TO_STR_HOOK: &str = "\
class Object
  def to_str = \"o\"
end

label = \"R$\"
count = 2
p label + count
";
const KEYED_ATTR_ACCESSOR: &str = "\
class Integer
  attr_accessor :zz
end

price = 100
label = \"R$\"
p price + label
";
const KEYED_INCLUDE_PLUS: &str = "\
module OperandTypesKeyedPlus
  def +(other) = \"mixed\"
end

Integer.include OperandTypesKeyedPlus

price = 100
label = \"R$\"
begin
  p price + label
rescue TypeError => e
  p e.class
end
";
const KEYED_INCLUDE_UNRELATED: &str = "\
module OperandTypesKeyedUnrelated
  def zz = 1
end

Integer.include OperandTypesKeyedUnrelated

price = 100
label = \"R$\"
p price + label
";
const KEYED_REOPEN_INCLUDE: &str = "\
module OperandTypesKeyedCoerce
  def coerce(other) = [1, 1]
end

class String
  include OperandTypesKeyedCoerce
end

price = 100
label = \"R$\"
p price + label
";
const KEYED_DYNAMIC_MIXIN_ARG: &str = "\
String.include(Module.new { def coerce(other) = [1, 1] })

price = 100
label = \"R$\"
p price + label
";
const KEYED_UNKNOWN_MACRO: &str = "\
class Integer
  instance_methods
end

price = 100
label = \"R$\"
begin
  p price + label
rescue TypeError => e
  p e.class
end
";
const KEYED_DYNAMIC_DEFINE_METHOD: &str = "\
class Integer
  define_method(:\"zz#{1}\") { 1 }
end

price = 100
label = \"R$\"
begin
  p price + label
rescue TypeError => e
  p e.class
end
";
const KEYED_REFINE_UNRELATED: &str = "\
module OperandTypesKeyedRefine
  refine Integer do
    def zz = 1
  end
end

using OperandTypesKeyedRefine

price = 100
label = \"R$\"
p price + label
";
const KEYED_MM_ON_ARG: &str = "\
class String
  def method_missing(name, *args) = [1, 1]

  def respond_to_missing?(name, priv = false) = true
end

price = 100
label = \"R$\"
p price + label
";
const KEYED_MM_ON_RECEIVER: &str = "\
class Integer
  def method_missing(name, *args) = 1
end

price = 100
label = \"R$\"
p price + label
";
const KEYED_ALIAS_REOPENING: &str = "\
OperandTypesKeyedAlias = Integer

class OperandTypesKeyedAlias
  def +(other) = \"aliased\"
end

price = 100
label = \"R$\"
p price + label
";
const KEYED_BLOCK_INCLUDE: &str = "\
module OperandTypesKeyedHelper
  def coerce(other) = [1, 1]
end

class OperandTypesKeyedHost
  def self.configure(&blk) = class_eval(&blk)
end

OperandTypesKeyedHost.configure do
  include OperandTypesKeyedHelper
end

price = 100
label = \"R$\"
p price + label
";

/// The reopening source, both sides: a body that defines `zz` cannot
/// change `1 + "s"` (MRI raises, and so do we now), a body that defines
/// `+` can (MRI prints `"keyed"`).
#[test]
fn a_core_reopening_is_read_by_name() {
    assert_eq!(
        check_source("keyed_reopen_unrelated.rb", KEYED_REOPEN_UNRELATED, true),
        vec!["7:11:error[E0108]: `+` on Integer expects a numeric operand, got String"],
    );
    assert_eq!(
        check_source("keyed_reopen_plus_silent.rb", KEYED_REOPEN_PLUS, true),
        Vec::<String>::new(),
    );
}

/// The receiver side is the class's OWN name, which is MRI's rule, not a
/// simplification: `class Object; def +(other); end` leaves `1 + "s"`
/// raising because `Integer#+` is found first, so it must not silence
/// the site — while `Object#coerce`, which the operator really does
/// consult on the ARGUMENT, must.
#[test]
fn an_ancestors_operator_does_not_silence_but_its_coercion_hook_does() {
    assert_eq!(
        check_source("keyed_object_plus_still_accuses.rb", KEYED_OBJECT_PLUS, true),
        vec!["7:11:error[E0108]: `+` on Integer expects a numeric operand, got String"],
    );
    assert_eq!(
        check_source("keyed_object_coerce_silent.rb", KEYED_OBJECT_COERCE, true),
        Vec::<String>::new(),
    );
}

/// The hook is the one THIS operator consults: `String#+` converts its
/// argument with `to_str`, so `Object#to_str` silences `"R$" + 2`.
#[test]
fn the_coercion_hook_is_per_pairing() {
    assert_eq!(
        check_source("keyed_to_str_hook_silent.rb", KEYED_TO_STR_HOOK, true),
        Vec::<String>::new(),
    );
}

/// Literal definer macros are read like a `def`: `attr_accessor :zz`
/// adds `zz`/`zz=` and nothing else, so the site keeps accusing.
#[test]
fn a_literal_definer_macro_is_read_by_name() {
    assert_eq!(
        check_source("keyed_attr_accessor_accuses.rb", KEYED_ATTR_ACCESSOR, true),
        vec!["7:11:error[E0108]: `+` on Integer expects a numeric operand, got String"],
    );
}

const KEYED_BLOCK_ATTR: &str = "\
Integer.class_eval do
  attr_accessor :zz
end

p 1 + \"s\"
";

/// Literal definer macros are read inside an eval BLOCK too, which is
/// where that reader is load-bearing (a `refine`/`class_eval` body has
/// no fragment to read instead): `attr_accessor :zz` adds `zz`/`zz=`,
/// so the operator is untouched and MRI keeps raising. The operands are
/// literals on purpose — with locals, the toplevel `class_eval` would
/// silence the site through `OperandLocalScan`'s bail and the fixture
/// would prove the locals map instead (measured).
#[test]
fn a_literal_definer_macro_inside_an_eval_block_is_read_by_name() {
    assert_eq!(
        check_source("keyed_block_attr_accuses.rb", KEYED_BLOCK_ATTR, true),
        vec!["5:7:error[E0108]: `+` on Integer expects a numeric operand, got String"],
    );
}

/// A mixin's method set is read through the module: `Integer.include M`
/// where `M` defines `zz` cannot change the operator.
///
/// The `+`-defining half is a measured ACCEPTED FALSE NEGATIVE, not a
/// silence MRI agrees with: `include` inserts the module BELOW the
/// class, so `Integer#+` still wins and MRI raises (the fixture rescues
/// it, which is what `Mri::Rescued` records). Distinguishing `include`
/// from `prepend` here would buy that one back; it is deliberately not
/// done, because invariant #1 tolerates the false negative and the
/// mixin-precedence rule is one more thing to keep right.
#[test]
fn a_mixins_method_set_is_read_by_name() {
    assert_eq!(
        check_source("keyed_include_unrelated_accuses.rb", KEYED_INCLUDE_UNRELATED, true),
        vec!["9:11:error[E0108]: `+` on Integer expects a numeric operand, got String"],
    );
    assert_eq!(
        check_source("keyed_include_plus.rb", KEYED_INCLUDE_PLUS, true),
        Vec::<String>::new(),
    );
}

/// `class String; include M; end` — the mixin read through the merged
/// fragment rather than through an injection call. `M#coerce` really
/// makes `100 + "R$"` print `2` under MRI.
#[test]
fn a_mixin_inside_a_core_reopening_is_read_too() {
    assert_eq!(
        check_source("keyed_reopen_include_silent.rb", KEYED_REOPEN_INCLUDE, true),
        Vec::<String>::new(),
    );
}

/// Fail-closed, one class at a time: a module argument that is not a
/// constant path (`String.include(Module.new { ... })`) is a method set
/// nobody can read, so `String` stands down for every name — measured
/// clean under MRI, where that module's `coerce` really runs.
#[test]
fn an_unreadable_mixin_argument_stands_its_own_class_down() {
    assert_eq!(
        check_source("keyed_dynamic_mixin_arg_silent.rb", KEYED_DYNAMIC_MIXIN_ARG, true),
        Vec::<String>::new(),
    );
}

/// The two fail-closed arms inside a core-class body: a macro this
/// checker does not model, and a `define_method` whose name is computed.
/// Both really raise under MRI (the fixtures rescue it), so both are
/// accepted false negatives — the price of not guessing what an
/// unmodeled macro defines.
#[test]
fn an_unreadable_core_body_statement_stands_that_class_down() {
    for (name, src) in [
        ("keyed_unknown_macro_fail_closed.rb", KEYED_UNKNOWN_MACRO),
        ("keyed_dynamic_define_method.rb", KEYED_DYNAMIC_DEFINE_METHOD),
    ] {
        assert_eq!(check_source(name, src, true), Vec::<String>::new(), "source: {name}");
    }
}

/// A refinement is read by name too, which turns the round-3 false
/// negative into a true positive: `refine Integer do def zz` leaves
/// `1 + "s"` raising under MRI, and the site now says so.
#[test]
fn a_refinement_that_defines_nothing_relevant_still_accuses() {
    assert_eq!(
        check_source("keyed_refine_unrelated_accuses.rb", KEYED_REFINE_UNRELATED, true),
        vec!["11:11:error[E0108]: `+` on Integer expects a numeric operand, got String"],
    );
}

const KEYED_REFINE_MACRO: &str = "\
module OperandTypesKeyedRefineMacro
  refine Integer do
    instance_methods
  end
end

using OperandTypesKeyedRefineMacro

price = 100
label = \"R$\"
begin
  p price + label
rescue TypeError => e
  p e.class
end
";

/// The `Opaque` fallback inside a REFINE body, where it is the only
/// reader there is: a refinement creates no fragment, so an unmodeled
/// macro in its block cannot be read from the index either. MRI really
/// raises (the fixture rescues it), so this is fail-closed silence, not
/// agreement.
#[test]
fn an_unreadable_refine_body_statement_stands_its_class_down() {
    assert_eq!(
        check_source("keyed_refine_unknown_macro.rb", KEYED_REFINE_MACRO, true),
        Vec::<String>::new(),
    );
}

/// `method_missing` on the ARGUMENT's class answers for the missing
/// conversion — measured: with `respond_to_missing?` alongside it,
/// `100 + "R$"` prints `2`. On the RECEIVER's class it changes nothing,
/// because `Integer#+` exists and is found first, and MRI keeps raising.
#[test]
fn method_missing_is_read_on_the_side_that_dispatches() {
    assert_eq!(
        check_source("keyed_method_missing_on_arg_silent.rb", KEYED_MM_ON_ARG, true),
        Vec::<String>::new(),
    );
    assert_eq!(
        check_source("keyed_method_missing_on_receiver.rb", KEYED_MM_ON_RECEIVER, true),
        vec!["7:11:error[E0108]: `+` on Integer expects a numeric operand, got String"],
    );
}

const KEYED_BLOCK_ARG_UNNAMABLE: &str = "\
obj = Object.new
blk = proc { 1 }
obj.instance_exec(&blk)

p 1 + \"s\"
";

/// The limit this change deliberately keeps: a BLOCK evaled into a
/// receiver nobody can name records nothing, so the site keeps accusing
/// — and MRI agrees here (it raises on line 5). Treating it as
/// fail-closed instead was measured against mastodon and reverted:
/// `obj.instance_exec(&blk)` is ordinary Ruby, and it stood every core
/// class down project-wide. The shipped blanket collector recorded
/// nothing for that shape either, so this keeps the keyed question from
/// being STRICTER than the blanket one it refines.
#[test]
fn a_block_evaled_into_an_unnamable_receiver_still_accuses() {
    assert_eq!(
        check_source("keyed_block_arg_unnamable_accuses.rb", KEYED_BLOCK_ARG_UNNAMABLE, true),
        vec!["5:7:error[E0108]: `+` on Integer expects a numeric operand, got String"],
    );
}

/// A reopening through a constant ALIAS is the same reopening:
/// `A = Integer; class A; def +(other)` prints `"aliased"` under MRI.
/// Reached by the fragment half of the collector, since the file scan
/// filters on core names and `A` is not one.
#[test]
fn a_reopening_through_an_alias_is_read_by_name() {
    assert_eq!(
        check_source("keyed_alias_reopening_silent.rb", KEYED_ALIAS_REOPENING, true),
        Vec::<String>::new(),
    );
}

const KEYED_TOPLEVEL_MM: &str = "\
def method_missing(name, *args) = 1

price = 100
label = \"R$\"
begin
  p price + label
rescue TypeError => e
  p e.class
end
";

/// A toplevel `def method_missing` lands on `Object`, which is in every
/// argument's ancestry, so it stands the pairing down — measured: the
/// program really raises (the fixture rescues it), so this is the
/// fail-closed side of the same missing-method read, not a silence MRI
/// agrees with.
#[test]
fn a_toplevel_method_missing_stands_every_class_down() {
    assert_eq!(
        check_source("keyed_toplevel_method_missing.rb", KEYED_TOPLEVEL_MM, true),
        Vec::<String>::new(),
    );
}

/// `include` inside a BLOCK is not a top-level include: the block may be
/// `class_eval`ed into any class, and reading it as an `Object` mixin
/// stood `Object` — hence every core class — down on all three public
/// corpora (measured: 81 phantom `Object` methods on mastodon). MRI
/// raises here, and the site accuses.
#[test]
fn an_include_inside_a_block_does_not_poison_object() {
    assert_eq!(
        check_source("keyed_block_include_still_accuses.rb", KEYED_BLOCK_INCLUDE, true),
        vec!["15:11:error[E0108]: `+` on Integer expects a numeric operand, got String"],
    );
}
