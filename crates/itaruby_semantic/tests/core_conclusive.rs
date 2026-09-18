//! Closed-world conclusive core lookup (bead ita-2ve): an unknown method
//! on a concrete core receiver (`Ty::Str` via narrowing or an ivar) is
//! E0101 only when (a) `ClosedWorld` is wired on — here explicitly, since
//! discovery lives in the binary (`main.rs::gems_detected`); (b) the
//! project never reopened the class; (c) the method is in neither the
//! `core.rs` allowlist nor the generated inventory
//! (`declarations/core_inventory.txt`). Fixtures live under
//! `testdata/core_conclusive/`, each with globally unique class names —
//! `testdata/` is scanned as a single merged project by `ita check
//! testdata/` (gate c). The Gemfile condition ((v) in the bead contract)
//! is proven end-to-end against the real binary by
//! `crates/itaruby/tests/core_closed_world_cli.rs`, because gem
//! discovery happens in the binary, not the library.

use itaruby_semantic::{check_file, ClosedWorld, Db, ProjectFiles, Severity, SourceFile};

type Diags = Vec<String>;

/// Check one fixture file in its OWN project with closed-world ON.
fn check_closed(name: &str) -> Diags {
    check_with(name, true)
}

/// Check one fixture file in its own project, closed-world off — v0
/// behavior, the library-side default.
fn check_open(name: &str) -> Diags {
    check_with(name, false)
}

fn check_with(name: &str, closed: bool) -> Diags {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/core_conclusive");
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

/// Inline source, own project, closed-world on — for the shapes that must
/// not live in `testdata/` (a String reopening would leak into every
/// other fixture through the merged project's `by_path`).
fn check_inline_closed(text: &str) -> Diags {
    let db = Db::default();
    let file = SourceFile::new(&db, "inline/core_conclusive.rb".into(), text.to_string());
    ProjectFiles::new(&db, vec![file]);
    ClosedWorld::new(&db, true);
    check_file(&db, file)
        .iter()
        .map(|d| format!("{}[{}]: {}", if d.severity == Severity::Error { "error" } else { "warning" }, d.code, d.message))
        .collect()
}

/// (i) FIRES: `x.pushh(1)` inside `if x.is_a?(String)` — the p5 A/B
/// gap. Narrowing types x as String, the method misses allowlist and
/// inventory, world is closed -> E0101 naming the receiver.
#[test]
fn narrowing_core_typo_fires_e0101() {
    let diags = check_closed("narrowing_core_typo.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0101"), "expected E0101, got: {diags:?}");
    assert!(diags[0].contains("pushh"), "got: {diags:?}");
    assert!(diags[0].contains("String"), "receiver must be named, got: {diags:?}");
}

/// (ii) FIRES: `@tag.push(1)` on an ivar typed String in initialize —
/// the p3 A/B gap.
#[test]
fn ivar_core_typo_fires_e0101() {
    let diags = check_closed("ivar_core_typo.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(diags[0].contains("E0101"), "expected E0101, got: {diags:?}");
    assert!(diags[0].contains("push"), "got: {diags:?}");
    assert!(diags[0].contains("String"), "receiver must be named, got: {diags:?}");
}

/// (iii) SILENT: `center` misses the allowlist but hits the generated
/// inventory — the inventory is what keeps the closed world from firing
/// on real methods. Also proves `center` really is outside the allowlist
/// (otherwise the fixture would be silent for the wrong reason).
#[test]
fn core_real_method_outside_allowlist_stays_silent() {
    assert!(
        itaruby_semantic::core::core_method(itaruby_semantic::core::CoreClass::Str, "center")
            .is_none(),
        "fixture premise: center is not allowlisted"
    );
    let diags = check_closed("core_real_method_silent.rb");
    assert!(diags.is_empty(), "real core method must stay silent, got: {diags:?}");
}

/// (iv) SILENT: the project reopens Array, so Array's conclusive lookup
/// stands down even though the method is unknown to allowlist+inventory.
#[test]
fn reopened_core_class_stays_silent() {
    let diags = check_closed("reopened_core_silent.rb");
    assert!(diags.is_empty(), "reopened core class must stay silent, got: {diags:?}");
}

/// (iv, spec's letter) SILENT in isolation: reopening String kills the
/// String conclusive lookup, but NOT Integer's — pollution is per class
/// ancestry, not global. Lives here (inline, own project) because a
/// String reopening inside `testdata/` would silence every String-typo
/// fixture in the merged gate-c run.
#[test]
fn reopened_string_silences_string_but_not_integer() {
    let diags = check_inline_closed(
        "class String\n  def core_inline_shout\n    \"loud\"\n  end\nend\n\
         class CoreInlineUser\n  def m(x)\n    if x.is_a?(String)\n      x.core_inline_shout\n    end\n  end\n\
         def n(y)\n    if y.is_a?(Integer)\n      y.core_inline_typo(1)\n    end\n  end\nend\n",
    );
    assert_eq!(diags.len(), 1, "exactly the Integer typo fires, got: {diags:?}");
    assert!(diags[0].contains("E0101") && diags[0].contains("core_inline_typo"), "got: {diags:?}");
}

/// (a) OFF by default: identical typo, no `ClosedWorld` wired — the
/// library's v0 behavior (and `ita server`/LSP/corpora forever).
#[test]
fn same_typo_is_silent_without_closed_world() {
    let diags = check_open("narrowing_core_typo.rb");
    assert!(diags.is_empty(), "no ClosedWorld input = v0 silence, got: {diags:?}");
}

/// Condition (b), metaprogramming shape: `String.prepend(M)` at top level
/// creates no fragment, so the walker flags `core_mixin` and the whole
/// conclusive path stands down — the code below is CORRECT (M really adds
/// pushh to String), and a conclusive E0101 here would be a false
/// positive, exactly what bead ita-d0j's dynamic-include gap taught.
#[test]
fn top_level_prepend_into_core_stays_silent() {
    let diags = check_inline_closed(
        "module CoreInlinePatch\n  def pushh(v)\n    self\n  end\nend\n\
         String.prepend(CoreInlinePatch)\n\
         class CoreInlinePrepended\n  def m(x)\n    if x.is_a?(String)\n      x.pushh(1)\n    end\n  end\nend\n",
    );
    assert!(diags.is_empty(), "core patched via prepend must stay silent, got: {diags:?}");
}

/// Condition (b), Object-level shape: top-level `include M` mixes into
/// Object, so every core receiver can gain M's methods — conclusive
/// lookup stands down.
#[test]
fn top_level_include_stays_silent() {
    let diags = check_inline_closed(
        "module CoreInlineObjPatch\n  def core_inline_everywhere; 1; end\nend\n\
         include CoreInlineObjPatch\n\
         class CoreInlineIncluder\n  def m\n    \"s\".core_inline_typo\n  end\nend\n",
    );
    assert!(diags.is_empty(), "top-level include must stand down, got: {diags:?}");
}

/// `x.extend(M)` on a local widens it to Unknown under closed-world: the
/// singleton extension can add any method, so the typo after it is a
/// false positive and must not fire.
#[test]
fn extend_on_local_widens_to_unknown() {
    let diags = check_inline_closed(
        "module CoreInlineExt\n  def pushh(v); self; end\nend\n\
         class CoreInlineExtender\n  def m\n    x = \"s\"\n    x.extend(CoreInlineExt)\n    x.pushh(1)\n  end\nend\n",
    );
    assert!(diags.is_empty(), "extend-widened local must stay silent, got: {diags:?}");
}

/// Narrowing to a core class works for the whole receiver set the
/// inventory covers — Integer via `is_a?` concludes too (message names
/// `Integer`), proving the path is not String-specific.
#[test]
fn integer_narrowing_typo_fires_too() {
    let diags = check_inline_closed(
        "class CoreInlineCounter\n  def m(y)\n    if y.is_a?(Integer)\n      y.core_inline_typo(1)\n    end\n  end\nend\n",
    );
    assert_eq!(diags.len(), 1, "got: {diags:?}");
    assert!(diags[0].contains("E0101") && diags[0].contains("Integer"), "got: {diags:?}");
}

/// Inventory parser: real methods present (across both Bool classes and
/// Object), typos absent. These are the only public faces of the parser,
/// so this doubles as the "header comments skipped" guard — a `#`-line
/// misparsed as data would surface as a bogus entry only if it matched a
/// `Class#method` lookup exactly, which these assertions pin down.
#[test]
fn inventory_knows_real_methods_and_rejects_typos() {
    use itaruby_semantic::core::core_inventory_has as has;
    for (class, method) in [
        ("String", "center"),
        ("String", "upcase"),
        ("Integer", "times"),
        ("Array", "each"),
        ("Hash", "each"),
        ("Symbol", "to_proc"),
        ("NilClass", "to_a"),
        ("TrueClass", "to_s"),
        ("FalseClass", "to_s"),
        ("Object", "inspect"),
    ] {
        assert!(has(class, method), "inventory must contain {class}#{method}");
    }
    for (class, method) in [
        ("String", "pushh"),
        ("Integer", "nmae"),
        ("Array", "srot"),
        ("Object", "totally_made_up"),
    ] {
        assert!(!has(class, method), "inventory must NOT contain {class}#{method}");
    }
}

// -- the blanket pollution collectors, from the side that reads them ----
//
// `refined_core`/`refined_unknown`/`eval_polluted_core`/
// `eval_polluted_unknown` answer the BLANKET question — "was this class
// touched at all?" — which is exactly what the conclusive core lookup
// needs and NOT what E0108 needs (that one asks after the operator by
// name, `check.rs`'s `core_ops_unpolluted`). Until round 5 both
// diagnostics read the blanket fields, so the E0108 suite doubled as
// their control; now that E0108 reads the keyed maps instead, these are
// the tests that keep the blanket fields honest. Every one of them is a
// program that really runs: a refinement or an eval really can add the
// method the typo names.

/// A refinement of `Integer` — EMPTY on purpose, since the blanket
/// question is "touched", not "touched how" — stands the conclusive
/// lookup down. The control below proves the same file fires without it.
#[test]
fn a_refinement_stands_the_conclusive_lookup_down() {
    let diags = check_inline_closed(
        "module CoreInlineRefine
  refine Integer do
  end
end
         class CoreInlineRefined
  def m(y)
    if y.is_a?(Integer)
      y.core_inline_typo(1)
    end
  end
end
",
    );
    assert!(diags.is_empty(), "a refined class cannot be concluded, got: {diags:?}");
}

/// The control for every fixture in this section: the same typo, with no
/// refinement and no eval, really does fire.
#[test]
fn the_same_typo_without_pollution_fires() {
    let diags = check_inline_closed(
        "class CoreInlineUnpolluted
  def m(y)
    if y.is_a?(Integer)
      y.core_inline_typo(1)
    end
  end
end
",
    );
    assert_eq!(diags.len(), 1, "got: {diags:?}");
    assert!(diags[0].contains("E0101"), "got: {diags:?}");
}

/// Per-class, not blanket: refining `Array` leaves an `Integer` typo
/// firing.
#[test]
fn refining_an_unrelated_class_leaves_the_lookup_conclusive() {
    let diags = check_inline_closed(
        "module CoreInlineRefineArr
  refine Array do
  end
end
         class CoreInlineRefinedArr
  def m(y)
    if y.is_a?(Integer)
      y.core_inline_typo(1)
    end
  end
end
",
    );
    assert_eq!(diags.len(), 1, "got: {diags:?}");
    assert!(diags[0].contains("E0101"), "got: {diags:?}");
}

/// A refinement through a constant ALIAS is the same refinement:
/// `A = Integer; refine A` stands `Integer` down.
#[test]
fn a_refinement_through_an_alias_stands_its_class_down() {
    let diags = check_inline_closed(
        "CoreInlineAlias = Integer
module CoreInlineRefineAlias
  refine CoreInlineAlias do
  end
end
         class CoreInlineRefinedAlias
  def m(y)
    if y.is_a?(Integer)
      y.core_inline_typo(1)
    end
  end
end
",
    );
    assert!(diags.is_empty(), "an aliased refinement target counts, got: {diags:?}");
}

/// A refinement target that cannot be named stands EVERY core class
/// down — the fail-closed arm.
#[test]
fn an_unnamable_refinement_stands_every_class_down() {
    let diags = check_inline_closed(
        "klass = Integer
module CoreInlineRefineDyn
  refine klass do
  end
end
         class CoreInlineRefinedDyn
  def m(y)
    if y.is_a?(Integer)
      y.core_inline_typo(1)
    end
  end
end
",
    );
    assert!(diags.is_empty(), "unknown refinement target must stand down, got: {diags:?}");
}

/// A string eval into a named core receiver stands that class down, and
/// only that class. Both halves put the eval inside a METHOD body on
/// purpose: a toplevel `Integer.class_eval(...)` is also a
/// `core_injection_call`, and THAT mark is receiver-blind by design
/// (measured here: a toplevel `Array.class_eval` silences an `Integer`
/// typo too), so a toplevel fixture would prove the wrong field.
#[test]
fn a_string_eval_stands_its_receiver_down() {
    let silent = check_inline_closed(
        "module CoreInlineEvalInstall
  def self.install
    Integer.class_eval(\"def core_inline_typo(v) = v\")\n  end
  install
end
\
         class CoreInlineEvaled\n  def m(y)\n    if y.is_a?(Integer)\n      y.core_inline_typo(1)\n    end\n  end\nend\n",
    );
    assert!(silent.is_empty(), "evaled receiver must stand down, got: {silent:?}");
    let fires = check_inline_closed(
        "module CoreInlineEvalArrInstall
  def self.install
    Array.class_eval(\"def core_inline_typo(v) = v\")\n  end
  install
end
\
         class CoreInlineEvaledArr\n  def m(y)\n    if y.is_a?(Integer)\n      y.core_inline_typo(1)\n    end\n  end\nend\n",
    );
    assert_eq!(fires.len(), 1, "evaling into Array leaves Integer conclusive, got: {fires:?}");
}

/// The same eval through a constant alias, and through a receiver that
/// cannot be named at all — the second stands every class down.
#[test]
fn an_aliased_or_unnamable_eval_receiver_stands_down() {
    let aliased = check_inline_closed(
        "CoreInlineEvalAlias = Integer
         CoreInlineEvalAlias.class_eval(\"def core_inline_typo(v) = v\")\n         class CoreInlineEvaledAlias
  def m(y)
    if y.is_a?(Integer)
      y.core_inline_typo(1)
    end
  end
end
",
    );
    assert!(aliased.is_empty(), "aliased eval receiver counts, got: {aliased:?}");
    let dynamic = check_inline_closed(
        "klass = Integer
src = \"def core_inline_typo(v) = v\"\nklass.class_eval(src)
         class CoreInlineEvaledDyn
  def m(y)
    if y.is_a?(Integer)
      y.core_inline_typo(1)
    end
  end
end
",
    );
    assert!(dynamic.is_empty(), "unnamable eval receiver must stand down, got: {dynamic:?}");
}

/// A bare `eval` names no receiver and its string can define anything
/// anywhere, so it stands every core class down.
#[test]
fn a_bare_eval_stands_every_class_down() {
    let diags = check_inline_closed(
        "eval(\"class Integer; def core_inline_typo(v) = v; end\")\n         class CoreInlineBareEval
  def m(y)
    if y.is_a?(Integer)
      y.core_inline_typo(1)
    end
  end
end
",
    );
    assert!(diags.is_empty(), "a bare eval must stand down, got: {diags:?}");
}
