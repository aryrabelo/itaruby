//! W11 (Tapioca open world): E0101 on a core receiver in a Bundler project
//! is licensed only by COMPLETE evidence of what every gem adds to the
//! receiver's method chain. Two accusing fixtures prove the mechanism
//! fires; every other fixture is one fail-closed condition, silent, and
//! each is accused by a named mutant in `scripts/open-world-mutants.sh`
//! (the mutant re-emits the exact E0101 the accusing twin pins).
//!
//! Tempdir projects, like `tapioca_closed_world_cli.rs`: a `Gemfile.lock`
//! inside `testdata/` would change what every whole-tree run discovers.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// `mygem` (with an RBI) plus `sorbet-runtime`, a Tapioca-ignored gem whose
/// locked version is source-audited (`open_world::CURATED_GEMS`).
const LOCK: &str = "GEM\n  remote: https://rubygems.org/\n  specs:\n    mygem (1.0.0)\n    sorbet-runtime (0.6.13437)\n\nPLATFORMS\n  ruby\n\nDEPENDENCIES\n  mygem\n  sorbet-runtime\n";
const RBI: &str = "# typed: true\n\nclass MyGem\n  def run; end\nend\n";
/// A `sig`-declared Integer whose caller calls a String method: Sorbet 7003,
/// MRI `NoMethodError`.
const SIG_INTEGER: &str = "# typed: true\nrequire \"sorbet-runtime\"\n\nclass A\n  extend T::Sig\n\n  sig { returns(Integer) }\n  def f = Integer(\"1\")\nend\n\nA.new.f.upcase\n";
/// The same shape through a declared, NON-final type.
const SIG_STRING: &str = "# typed: true\nrequire \"sorbet-runtime\"\n\nclass A\n  extend T::Sig\n\n  sig { returns(String) }\n  def f = String.new(\"x\")\nend\n\nA.new.f.upcase_all\n";

fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("open-world")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    let base = [
        (
            "Gemfile",
            "source \"https://rubygems.org\"\ngem \"mygem\"\ngem \"sorbet-runtime\"\n",
        ),
        ("Gemfile.lock", LOCK),
        ("sorbet/rbi/gems/mygem@1.0.0.rbi", RBI),
    ];
    let store = gem_store(&dir);
    let _ = std::fs::remove_dir_all(&store);
    for (rel, text) in base
        .iter()
        .filter(|(rel, _)| !files.iter().any(|(r, _)| r == rel))
        .chain(files)
    {
        // By default `mygem`'s installed source says exactly what its RBI
        // says (the RBI's `sig`s are valid Ruby calls); `NO_SOURCE` leaves
        // it uninstalled. `@gems/<rel>` writes into the gem store.
        if *rel == "sorbet/rbi/gems/mygem@1.0.0.rbi" && !files.iter().any(|(r, _)| *r == NO_SOURCE)
        {
            let lib = store.join("gems/mygem-1.0.0/lib/mygem.rb");
            std::fs::create_dir_all(lib.parent().unwrap()).unwrap();
            std::fs::write(lib, text).unwrap();
        }
        if *rel == NO_SOURCE {
            continue;
        }
        let path = match rel.strip_prefix("@gems/") {
            Some(gem_rel) => store.join("gems").join(gem_rel),
            None => dir.join(rel),
        };
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        if !text.is_empty() {
            std::fs::write(path, text).unwrap();
        }
    }
    dir
}

/// Marker file entry: do not install `mygem`'s source.
const NO_SOURCE: &str = "!no-source";

/// The fixture's private installed-gem directory (`GEM_PATH`), a sibling
/// of the project so the unloaded-file harvest never walks it.
fn gem_store(dir: &Path) -> PathBuf {
    dir.with_extension("gems")
}

fn check(dir: &Path) -> (String, Option<i32>) {
    let out: Output = Command::new(env!("CARGO_BIN_EXE_ita"))
        .env("GEM_PATH", gem_store(dir))
        .env_remove("GEM_HOME")
        .arg("check")
        .arg(".")
        .current_dir(dir)
        .output()
        .expect("run `ita check`");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code(),
    )
}

const INTEGER_UPCASE: &str =
    "./main.rb:11:9: error[E0101]: undefined method `upcase` for `Integer`";

fn assert_silent(name: &str, files: &[(&str, &str)]) {
    let dir = project(name, files);
    let (text, code) = check(&dir);
    assert!(
        !text.contains("error[") && !text.contains("warning["),
        "{name}: fail closed to silence, got:\n{text}"
    );
    assert_eq!(code, Some(0), "{name}: no errors expected, got:\n{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- the mechanism fires -----------------------------------------------

#[test]
fn sig_declared_integer_accuses_under_complete_evidence() {
    let dir = project("accuse-sig", &[("main.rb", SIG_INTEGER)]);
    let (text, code) = check(&dir);
    assert!(text.contains(INTEGER_UPCASE), "got:\n{text}");
    assert_eq!(code, Some(1), "got:\n{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn literal_receiver_is_exact_even_for_a_non_final_class() {
    let dir = project(
        "accuse-literal",
        &[("main.rb", "# typed: true\n\"s\".upcase_all\n1.upcase\n")],
    );
    let (text, code) = check(&dir);
    assert!(
        text.contains("./main.rb:2:5: error[E0101]: undefined method `upcase_all` for `String`"),
        "got:\n{text}"
    );
    assert!(
        text.contains("./main.rb:3:3: error[E0101]: undefined method `upcase` for `Integer`"),
        "got:\n{text}"
    );
    assert_eq!(code, Some(1));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn without_a_gem_rbi_tree_the_project_stays_open() {
    assert_silent(
        "open-no-rbis",
        &[
            ("main.rb", SIG_INTEGER),
            ("sorbet/rbi/gems/mygem@1.0.0.rbi", ""),
            (NO_SOURCE, ""),
        ],
    );
}

// ---- trap 1: declared type = exact class -------------------------------

/// `returns(String)` is satisfied by any subclass, and a subclass may
/// define the method (`ActiveSupport::SafeBuffer < String`).
#[test]
fn declared_non_final_type_stays_silent() {
    assert_silent(
        "silent-subclass",
        &[
            ("main.rb", SIG_STRING),
            (
                "sorbet/rbi/gems/mygem@1.0.0.rbi",
                "# typed: true\n\nclass SafeBuf < String\n  def upcase_all; end\nend\n",
            ),
        ],
    );
}

/// A gem class whose `is_a?` answers true for `Integer` passes the sig.
#[test]
fn rbi_type_test_override_stays_silent() {
    assert_silent(
        "silent-rbi-faker",
        &[
            ("main.rb", SIG_INTEGER),
            (
                "sorbet/rbi/gems/mygem@1.0.0.rbi",
                "# typed: true\n\nclass Fake\n  def is_a?(k); end\n  def upcase; end\nend\n",
            ),
        ],
    );
}

#[test]
fn project_type_test_override_stays_silent() {
    assert_silent(
        "silent-project-faker",
        &[
            ("main.rb", SIG_INTEGER),
            (
                "fake.rb",
                "class Fake\n  def kind_of?(k) = true\n  def upcase = \"1\"\nend\n",
            ),
        ],
    );
}

// ---- trap 2: complete evidence or nothing -------------------------------

#[test]
fn gem_without_rbi_stays_silent() {
    let lock = LOCK.replace(
        "    mygem (1.0.0)\n",
        "    mygem (1.0.0)\n    other (2.0.0)\n",
    );
    assert_silent(
        "silent-missing-rbi",
        &[("main.rb", SIG_INTEGER), ("Gemfile.lock", &lock)],
    );
}

#[test]
fn stale_rbi_version_stays_silent() {
    assert_silent(
        "silent-stale-rbi",
        &[
            ("main.rb", SIG_INTEGER),
            ("sorbet/rbi/gems/mygem@1.0.0.rbi", ""),
            (NO_SOURCE, ""),
            ("sorbet/rbi/gems/mygem@0.9.0.rbi", RBI),
        ],
    );
}

#[test]
fn unaudited_tapioca_ignored_version_stays_silent() {
    let lock = LOCK.replace("sorbet-runtime (0.6.13437)", "sorbet-runtime (0.6.99999)");
    assert_silent(
        "silent-unaudited",
        &[("main.rb", SIG_INTEGER), ("Gemfile.lock", &lock)],
    );
}

#[test]
fn path_gem_outside_the_project_stays_silent() {
    let lock =
        format!("PATH\n  remote: /nonexistent/w11-path-gem\n  specs:\n    local (0.1.0)\n\n{LOCK}");
    assert_silent(
        "silent-path-gem",
        &[("main.rb", SIG_INTEGER), ("Gemfile.lock", &lock)],
    );
}

/// A file `sorbet/config` keeps out of the run still runs. Since the
/// `--ignore` indexing (`OpenReason::IgnoredReopening`) also stands this
/// Integer down, the unloaded harvest alone is pinned by the next fixture.
#[test]
fn ignored_path_reopening_core_stays_silent() {
    assert_silent(
        "silent-ignored-path",
        &[
            ("main.rb", SIG_INTEGER),
            ("sorbet/config", "--dir\n.\n--ignore=legacy/\n"),
            (
                "legacy/patch.rb",
                "class Integer\n  def upcase = to_s\nend\n",
            ),
        ],
    );
}

/// The shape only the unloaded harvest reaches: an extensionless script
/// no run loads (`bin/`), outside every `--ignore`, reopening Integer.
#[test]
fn unloaded_script_reopening_core_stays_silent() {
    assert_silent(
        "silent-unloaded-script",
        &[
            ("main.rb", SIG_INTEGER),
            ("bin/patch", "class Integer\n  def upcase = to_s\nend\n"),
        ],
    );
}

#[test]
fn ignored_path_type_test_override_stays_silent() {
    assert_silent(
        "silent-ignored-faker",
        &[
            ("main.rb", SIG_INTEGER),
            ("sorbet/config", "--dir\n.\n--ignore=legacy/\n"),
            ("legacy/fake.rb", "class Fake\n  def is_a?(k) = true\nend\n"),
        ],
    );
}

// ---- trap 3: the whole ancestor chain -----------------------------------

/// A module included into an ancestor (`Comparable`) of the receiver: the
/// name lives on the included module, never on the reopening itself.
#[test]
fn rbi_module_included_into_comparable_stays_silent() {
    assert_silent(
        "silent-comparable-include",
        &[
            ("main.rb", SIG_INTEGER),
            (
                "sorbet/rbi/gems/mygem@1.0.0.rbi",
                "# typed: true\n\nmodule MyGem::Ext\n  def upcase; end\nend\n\nmodule Comparable\n  include ::MyGem::Ext\nend\n",
            ),
            ("@gems/mygem-1.0.0/lib/mygem.rb", "# the RBI is the only witness\n"),
        ],
    );
}

#[test]
fn rbi_reopening_basic_object_stays_silent() {
    assert_silent(
        "silent-basic-object",
        &[
            ("main.rb", SIG_INTEGER),
            (
                "sorbet/rbi/gems/mygem@1.0.0.rbi",
                "# typed: true\n\nclass BasicObject\n  def upcase; end\nend\n",
            ),
            (
                "@gems/mygem-1.0.0/lib/mygem.rb",
                "# the RBI is the only witness\n",
            ),
        ],
    );
}

#[test]
fn rbi_module_prepended_to_kernel_stays_silent() {
    assert_silent(
        "silent-kernel-prepend",
        &[
            ("main.rb", SIG_INTEGER),
            (
                "sorbet/rbi/gems/mygem@1.0.0.rbi",
                "# typed: true\n\nmodule MyGem::Ext\n  def upcase; end\nend\n\nmodule Kernel\n  prepend ::MyGem::Ext\nend\n",
            ),
            ("@gems/mygem-1.0.0/lib/mygem.rb", "# the RBI is the only witness\n"),
        ],
    );
}

#[test]
fn rbi_method_missing_on_object_stays_silent() {
    assert_silent(
        "silent-method-missing",
        &[
            ("main.rb", "# typed: true\n1.upcase\n"),
            (
                "sorbet/rbi/gems/mygem@1.0.0.rbi",
                "# typed: true\n\nclass Object\n  def method_missing(name, *args); end\nend\n",
            ),
            (
                "@gems/mygem-1.0.0/lib/mygem.rb",
                "# the RBI is the only witness\n",
            ),
        ],
    );
}

/// An RBI reopening whose body cannot be read whole answers every name.
#[test]
fn unreadable_rbi_reopening_stays_silent() {
    assert_silent(
        "silent-unreadable",
        &[
            ("main.rb", SIG_INTEGER),
            (
                "sorbet/rbi/gems/mygem@1.0.0.rbi",
                "# typed: true\n\nclass Integer\n  NAMES.each { |n| define_method(n) {} }\nend\n",
            ),
            (
                "@gems/mygem-1.0.0/lib/mygem.rb",
                "# the RBI is the only witness\n",
            ),
        ],
    );
}

#[test]
fn project_refinement_stays_silent() {
    assert_silent(
        "silent-refinement",
        &[
            ("main.rb", SIG_INTEGER),
            (
                "ref.rb",
                "module Up\n  refine Integer do\n    def upcase = to_s\n  end\nend\n",
            ),
        ],
    );
}

// ---- trap 4: stdlib and Tapioca-ignored gems ----------------------------

#[test]
fn stdlib_extension_stays_silent() {
    // `to_bn` (openssl), not `to_json`: the core inventory already carries
    // `to_json`, so a fixture on it would silence for another reason.
    assert_silent(
        "silent-stdlib",
        &[("main.rb", "# typed: true\nrequire \"openssl\"\n1.to_bn\n")],
    );
}

#[test]
fn curated_gem_name_stays_silent() {
    let lock = LOCK.replace(
        "    mygem (1.0.0)\n",
        "    debug (1.11.1)\n    mygem (1.0.0)\n",
    );
    assert_silent(
        "silent-curated",
        &[
            ("main.rb", "# typed: true\n1.trap\n"),
            ("Gemfile.lock", &lock),
        ],
    );
}

// ---- E0108 shares the chain ---------------------------------------------

#[test]
fn rbi_coercion_hook_keeps_e0108_silent() {
    assert_silent(
        "silent-coerce",
        &[
            ("main.rb", "# typed: true\na = 1\nb = \"x\"\na + b\n"),
            (
                "sorbet/rbi/gems/mygem@1.0.0.rbi",
                "# typed: true\n\nclass String\n  def coerce(other); end\nend\n",
            ),
        ],
    );
}

#[test]
fn integer_plus_string_accuses_under_complete_evidence() {
    let dir = project(
        "accuse-e0108",
        &[("main.rb", "# typed: true\na = 1\nb = \"x\"\na + b\n")],
    );
    let (text, _) = check(&dir);
    assert!(text.contains("error[E0108]"), "got:\n{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// E0107 reads project and core candidates only (`A#run`, `B#frobnicate`,
/// joined by no project class); under Tapioca evidence a gem object
/// answering both names is real, so E0107 stays off.
#[test]
fn gem_class_answering_both_names_keeps_e0107_off() {
    assert_silent(
        "silent-e0107",
        &[
            (
                "main.rb",
                "# typed: true\nclass A\n  def run; end\nend\n\nclass B\n  def frobnicate; end\nend\n\ndef g(x)\n  x.run\n  x.frobnicate\nend\n",
            ),
            (
                "sorbet/rbi/gems/mygem@1.0.0.rbi",
                "# typed: true\n\nclass MyGem\n  def run; end\n  def frobnicate; end\nend\n",
            ),
        ],
    );
}

// ---- the shapes Tapioca really writes ----------------------------------

/// Tapioca restates MRI's own inclusions (`include ::Comparable` in every
/// `class String` reopening) and writes `sig`s with generics this crate
/// does not parse. Neither defines a name: the reopening stays readable
/// and a literal typo still accuses. The second `sig` ends in `.checked`,
/// so neither the contract grammar nor the `returns`/`void` builder rule
/// recognizes it: it is the shape that reaches the class-body opening.
#[test]
fn tapioca_core_reopening_shape_stays_readable() {
    let dir = project(
        "accuse-tapioca-shape",
        &[
            ("main.rb", "# typed: true\n\"s\".upcase_all\n"),
            (
                "sorbet/rbi/gems/mygem@1.0.0.rbi",
                "# typed: true\n\nclass String\n  include ::Comparable\n\n  sig { type_parameters(:U).params(fill_with: T.type_parameter(:U)).returns(T::Array[T.any(Elem, T.type_parameter(:U))]) }\n  def in_groups_of(fill_with); end\n\n  sig { type_parameters(:U).params(x: T.type_parameter(:U)).returns(T.type_parameter(:U)).checked(:never) }\n  def presence_in(x); end\nend\n",
            ),
            (
                "@gems/mygem-1.0.0/lib/mygem.rb",
                "class String\n  def in_groups_of(fill_with); end\n  def presence_in(x); end\nend\n",
            ),
        ],
    );
    let (text, code) = check(&dir);
    assert!(
        text.contains("./main.rb:2:5: error[E0101]: undefined method `upcase_all` for `String`"),
        "got:\n{text}"
    );
    assert_eq!(code, Some(1));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Including a chain module MRI does not include there hands the receiver
/// names the inventory never lists (`1.map`): unreadable, silent.
#[test]
fn non_core_inclusion_of_a_chain_module_stays_silent() {
    assert_silent(
        "silent-noncore-include",
        &[
            ("main.rb", "# typed: true\n1.map\n"),
            (
                "sorbet/rbi/gems/mygem@1.0.0.rbi",
                "# typed: true\n\nclass Integer\n  include ::Enumerable\nend\n",
            ),
        ],
    );
}

// ---- installed gem source (Tapioca's RBI is not complete) ---------------

/// Accusing twin for the source-harvest fixtures: `text` must contain
/// exactly `expected` and exit 1.
fn assert_accuses(name: &str, files: &[(&str, &str)], expected: &str) {
    let dir = project(name, files);
    let (text, code) = check(&dir);
    assert!(
        text.contains(expected),
        "{name}: expected `{expected}`, got:\n{text}"
    );
    assert_eq!(code, Some(1), "{name}: one planted error, got:\n{text}");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(gem_store(&dir));
}

/// The measured `json_expressions` 0.9.0 shape: the gem does
/// `alias_method :ignore_extra_keys!, :forgiving!` inside `class Hash`,
/// and Tapioca's RBI lists only the `include`. The installed source is
/// what names the alias.
const ALIAS_RBI: &str = "# typed: true\n\nclass Hash\n  include ::MyGem::CoreExt\nend\n\nmodule MyGem::CoreExt\n  def forgiving!; end\nend\n";
const ALIAS_SRC: &str = "module MyGem\n  module CoreExt\n    def forgiving!; end\n  end\nend\n\nclass Hash\n  include MyGem::CoreExt\n  alias_method :ignore_extra_keys!, :forgiving!\nend\n";
const HASH_ALIAS_CALL: &str = "# typed: true\n{}.ignore_extra_keys!\n";

#[test]
fn source_alias_the_rbi_omits_stays_silent() {
    assert_silent(
        "silent-source-alias",
        &[
            ("main.rb", HASH_ALIAS_CALL),
            ("sorbet/rbi/gems/mygem@1.0.0.rbi", ALIAS_RBI),
            ("@gems/mygem-1.0.0/lib/mygem.rb", ALIAS_SRC),
        ],
    );
}

/// Without the source on disk, every chain namespace that RBI reopens
/// stays open - and ONLY those.
#[test]
fn missing_source_opens_the_class_its_rbi_reopens() {
    assert_silent(
        "silent-missing-source-hash",
        &[
            ("main.rb", HASH_ALIAS_CALL),
            ("sorbet/rbi/gems/mygem@1.0.0.rbi", ALIAS_RBI),
            (NO_SOURCE, ""),
        ],
    );
}

/// Positive side of the per-class rule: a gem whose RBI only reopens
/// `Hash` cannot give `Integer` a method.
#[test]
fn missing_source_for_a_hash_gem_leaves_integer_closed() {
    assert_accuses(
        "accuse-missing-source-integer",
        &[
            ("main.rb", SIG_INTEGER),
            ("sorbet/rbi/gems/mygem@1.0.0.rbi", ALIAS_RBI),
            (NO_SOURCE, ""),
        ],
        INTEGER_UPCASE,
    );
}

/// A gem with no RBI for its locked version is covered by its installed
/// source when that source is readable.
#[test]
fn gap_gem_with_clean_installed_source_is_covered() {
    assert_accuses(
        "accuse-gap-clean-source",
        &[
            ("main.rb", SIG_INTEGER),
            ("sorbet/rbi/gems/mygem@1.0.0.rbi", ""),
            ("sorbet/rbi/gems/mygem@0.9.0.rbi", RBI),
            (
                "@gems/mygem-1.0.0/lib/mygem.rb",
                "class MyGem\n  def run; end\nend\n",
            ),
        ],
        INTEGER_UPCASE,
    );
}

/// ... and what that source adds to the chain is read name-keyed.
#[test]
fn gap_gem_source_defining_the_name_stays_silent() {
    assert_silent(
        "silent-gap-source-defines",
        &[
            ("main.rb", SIG_INTEGER),
            ("sorbet/rbi/gems/mygem@1.0.0.rbi", ""),
            ("sorbet/rbi/gems/mygem@0.9.0.rbi", RBI),
            (
                "@gems/mygem-1.0.0/lib/mygem.rb",
                "class Integer\n  def upcase = to_s\nend\n",
            ),
        ],
    );
}

/// C code defines methods no walker reads: a gap gem with a native
/// extension stays a gap.
#[test]
fn gap_gem_with_native_extension_stays_silent() {
    assert_silent(
        "silent-gap-native",
        &[
            ("main.rb", SIG_INTEGER),
            ("sorbet/rbi/gems/mygem@1.0.0.rbi", ""),
            ("sorbet/rbi/gems/mygem@0.9.0.rbi", RBI),
            ("@gems/mygem-1.0.0/lib/mygem.rb", "class MyGem\nend\n"),
            (
                "@gems/mygem-1.0.0/ext/mygem/mygem.c",
                "/* rb_define_method(rb_cInteger, \"upcase\", ...) */\n",
            ),
        ],
    );
}

// ---- name-keyed core definers and top-level includes ---------------------

/// An unloaded script's top-level `include M` mixes M into `Object`; M's
/// methods are read from the same unloaded unit.
#[test]
fn unloaded_toplevel_include_of_a_readable_module_stays_silent() {
    assert_silent(
        "silent-unloaded-include",
        &[
            ("main.rb", SIG_INTEGER),
            (
                "bin/update",
                "module Helpers\n  def upcase = 'x'\nend\ninclude Helpers\n",
            ),
        ],
    );
}

/// The serveme shape: `include FileUtils` at top level of an unloaded
/// script. `FileUtils`'s public instance surface is empty (all
/// `module_function`), so nothing reaches an Integer.
#[test]
fn unloaded_toplevel_include_of_fileutils_still_accuses() {
    assert_accuses(
        "accuse-unloaded-fileutils",
        &[
            ("main.rb", SIG_INTEGER),
            ("bin/update", "require 'fileutils'\ninclude FileUtils\n"),
        ],
        INTEGER_UPCASE,
    );
}

/// The tapioca shape: `Kernel.define_method(:abort, ...)` names its
/// method, so it stands down that NAME only.
const KERNEL_DEFINE: &str = "Kernel.define_method(:frobnicate) { |*| nil }\n";

#[test]
fn literal_kernel_define_method_answers_its_name() {
    assert_silent(
        "silent-kernel-define-literal",
        &[
            ("main.rb", "# typed: true\n1.frobnicate\n"),
            ("lib/patch.rb", KERNEL_DEFINE),
        ],
    );
}

#[test]
fn literal_kernel_define_method_leaves_other_names_closed() {
    assert_accuses(
        "accuse-kernel-define-literal",
        &[("main.rb", SIG_INTEGER), ("lib/patch.rb", KERNEL_DEFINE)],
        INTEGER_UPCASE,
    );
}

/// A computed name may be any name: still the blanket stand-down.
#[test]
fn dynamic_kernel_define_method_stays_silent() {
    assert_silent(
        "silent-kernel-define-dynamic",
        &[
            ("main.rb", SIG_INTEGER),
            (
                "lib/patch.rb",
                "NAME = :upcase\nKernel.define_method(NAME) { |*| nil }\n",
            ),
        ],
    );
}

/// A gem WITH an RBI: what its code defined at load time through a
/// receiver no constant names is reflected in that RBI if it reached the
/// chain, so the source harvest reads only chain-targeted shapes
/// (`open_world::Reach::Targeted`).
#[test]
fn rbi_gem_untargeted_definer_is_left_to_its_rbi() {
    assert_accuses(
        "accuse-rbi-gem-untargeted",
        &[
            ("main.rb", SIG_INTEGER),
            ("sorbet/rbi/gems/mygem@1.0.0.rbi", "# typed: true\n\nclass Hash\n  def mygem_x; end\nend\n"),
            (
                "@gems/mygem-1.0.0/lib/mygem.rb",
                "class Hash\n  def mygem_x; end\nend\n\ndef self.install(k)\n  k.define_method(:upcase) { 1 }\nend\n",
            ),
        ],
        INTEGER_UPCASE,
    );
}

/// A gem with NO RBI has nothing that reflected it: the same definer may
/// land on Integer (`Reach::Everything`).
#[test]
fn gap_gem_untargeted_definer_stays_silent() {
    assert_silent(
        "silent-gap-untargeted",
        &[
            ("main.rb", SIG_INTEGER),
            ("sorbet/rbi/gems/mygem@1.0.0.rbi", ""),
            ("sorbet/rbi/gems/mygem@0.9.0.rbi", RBI),
            (
                "@gems/mygem-1.0.0/lib/mygem.rb",
                "def install(k)\n  k.define_method(:upcase) { 1 }\nend\n",
            ),
        ],
    );
}

/// A refinement of Integer that defines a DIFFERENT name: the blanket
/// `refined_core` read still stands Integer down (a refinement body is
/// activated by `using`, and the conclusive lookup does not track scope).
#[test]
fn project_refinement_of_another_name_stays_silent() {
    assert_silent(
        "silent-refinement-other",
        &[
            ("main.rb", SIG_INTEGER),
            (
                "ref.rb",
                "module Up\n  refine Integer do\n    def other_name = to_s\n  end\nend\n",
            ),
        ],
    );
}
