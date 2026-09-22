#!/usr/bin/env python3
"""Two-sided ancestry probes. Builds only a temporary source copy, never the checkout.

Run: python3 scripts/conflicting-superclasses-mutants.py
The baseline and each mutant run the focused integration test. Invalid/no-op
mutations, compiler failures, and failures of the wrong test are distinct verdicts.
"""
from pathlib import Path
import filecmp
import os
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
SOURCE = Path("crates/itaruby_semantic/src/index.rs")
TESTS = ("conflicting_superclasses", "ancestry_review_controls")
CONFLICT = "conflicting_bases_never_choose_an_inherited_method_or_constant"
GLOBAL = "ambiguous_ancestor_never_falls_back_to_a_global_class_or_alias"
NESTING = "a_superclass_header_keeps_its_own_nesting_after_a_bare_reopening"


def verdict(build_code, run_code, output, expected):
    if build_code != 0:
        return "INVALID-build"
    if run_code == 0:
        return "BLIND" if expected else "PASS"
    if expected:
        names = (expected,) if isinstance(expected, str) else expected
        if all(re.search(rf"^test {re.escape(name)} \.\.\. FAILED$", output, re.M) for name in names):
            return "CAUGHT"
    return "WRONG-check"


def run(copy, *args):
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(copy / "target")
    tests = [argument for test in TESTS for argument in ("--test", test)]
    result = subprocess.run(
        ["cargo", "test", "--locked", "--no-fail-fast", "-p", "itaruby_semantic", *tests, *args],
        cwd=copy, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        check=False,
    )
    print(result.stdout, end="", flush=True)
    return result


def exercise(copy, original, content, expected):
    target = copy / SOURCE
    target.write_text(content)
    if expected and filecmp.cmp(original, target, shallow=False):
        return "INVALID-cmp", None
    built = run(copy, "--no-run")
    if built.returncode != 0:
        return "INVALID-build", (built.returncode, None, built.stdout)
    tested = run(copy, "--", "--test-threads=1")
    evidence = (built.returncode, tested.returncode, tested.stdout)
    return verdict(*evidence, expected), evidence


def require(actual, expected, label):
    print(f"{label}: {actual}", flush=True)
    if actual != expected:
        raise SystemExit(f"{label}: expected {expected}, got {actual}")


def main():
    source = (ROOT / SOURCE).read_text()
    with tempfile.TemporaryDirectory(prefix="ita-ancestry-mutants-") as temporary:
        copy = Path(temporary)
        for name in ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml"]:
            shutil.copy2(ROOT / name, copy / name)
        shutil.copytree(ROOT / "crates", copy / "crates")
        # Cargo builds this semantic-crate bin for integration tests too.
        # Copy its public source explicitly, never the scripts directory
        # (which may contain machine-local corpus configuration).
        (copy / "scripts").mkdir()
        generator = Path("scripts/gen-activerecord-inventory.rs")
        shutil.copy2(ROOT / generator, copy / generator)
        original = copy / "original-index.rs"
        original.write_text(source)

        result, _ = exercise(copy, original, source, None)
        require(result, "PASS", "positive control")
        result, _ = exercise(copy, original, source, CONFLICT)
        require(result, "INVALID-cmp", "no-op guard")
        result, _ = exercise(copy, original, source + "\nnot valid Rust;\n", CONFLICT)
        require(result, "INVALID-build", "compiler guard")

        mutations = [
            ("first superclass wins", r"    reconcile_superclasses\(&mut index\);", "", CONFLICT),
            ("constant uncertainty lost", r"if ambiguous\s*\{\s*ConstResolution::Ambiguous\s*\} else\s*\{\s*ConstResolution::Missing\s*\}", "ConstResolution::Missing", ("intermediate_conflicting_namespace_keeps_relative_and_cbase_paths_unknown", "aliases_retain_uncertainty_from_intermediate_qualified_namespaces")),
            ("global fallback enabled", r"if self\.conflicting_const_scope\(nesting\) \{\s*return None;\s*\}\s*self\.by_path\.get\(name\)\.copied\(\)", "self.by_path.get(name).copied()", GLOBAL),
            ("alias fallback enabled", r"if self\.conflicting_const_scope\(nesting\) \{\s*return None;\s*\}\s*self\.const_aliases\.get\(name\)", "self.const_aliases.get(name)", (GLOBAL, "each_alias_hop_respects_its_write_scope_before_global_fallback")),
            ("qualified constant uncertainty lost", r"ConstResolution::Resolved\(_\) \| ConstResolution::Ambiguous => return true,\s*ConstResolution::Missing => \{\}", "ConstResolution::Resolved(_) => return true,\n            ConstResolution::Ambiguous | ConstResolution::Missing => {}", GLOBAL),
            ("header context discarded", r"let nesting = self\s*\.superclass_nesting\s*\.get\(&id\)\s*\.map_or\(nesting, Vec::as_slice\);", "", NESTING),
            ("scope identity ignored", r"name == first_name && nesting == first_nesting", "name == first_name", "identical_base_text_in_different_scopes_conflicts_but_local_methods_remain_known"),
            ("ancestry marked complete", r"\*complete &= !self\.ambiguous_ancestry\.contains\(&id\);", "*complete &= true;", CONFLICT),
            # Removing the header (not just blocking its resolver) protects
            # both method lookup and raw-spelling consumers such as schema.
            ("conflict-dependent superclass retained", r'!name\.starts_with\("::"\)\s*&& self\.conflicting_const_scope\(&nesting\[\.\.nesting\.len\(\)\.saturating_sub\(1\)\]\)\s*&& self\.resolve_superclass_lexical\(id, nesting, name\)\.is_none\(\)', "false", ("superclass_via_conflicting_enclosing_ancestry_has_no_global_winner", "schema_cannot_invent_attributes_from_an_enclosing_conflict")),
            ("schema synthesized before consensus", r"    reconcile_superclasses\(&mut index\);", "    if let Some(project) = ProjectFiles::try_get(db) { merge_schema_declarations(db, project, &mut index); }\n    reconcile_superclasses(&mut index);", "schema_synthesis_cannot_use_a_conflicting_superclass"),
            ("intermediate namespace uncertainty lost", r"Some\(id\) => self\.ambiguous_ancestry\.contains\(&id\),", "Some(_) => false,", "intermediate_conflicting_namespace_keeps_relative_and_cbase_paths_unknown"),
            ("Class fallback crosses conflict", r"if self\.ambiguous_ancestry\.contains\(&id\) \{\s*return Some\(MethodLookup::Inconclusive\);", "if false { return Some(MethodLookup::Inconclusive);", "class_reopening_is_not_a_fallback_past_conflicting_singleton_ancestry"),
            ("own constructor hidden by conflict", r"pub fn lookup_singleton_own\(&self, id: ClassId, name: &str\) -> MethodLookup<'_> \{", "pub fn lookup_singleton_own(&self, id: ClassId, name: &str) -> MethodLookup<'_> {\n        if self.ambiguous_ancestry.contains(&id) { return MethodLookup::Inconclusive; }", "directly_defined_new_keeps_its_arity_and_navigation_under_base_conflict"),
            ("alias RHS loses its write scope", r"let found = self\.resolve_const_path\(&alias\.0, &alias\.1, walk\);", "let found = self.resolve_const_path(&[], &alias.1, walk);", "each_alias_hop_respects_its_write_scope_before_global_fallback"),
            ("lexical alias hidden by entry barrier", r"if let Some\(alias\) = self\.find_const_alias\(nesting, name\) \{", "if self.conflicting_const_scope(nesting) { return ConstResolution::Ambiguous; }\n        if let Some(alias) = self.find_const_alias(nesting, name) {", "proven_lexical_alias_keeps_arity_and_navigation_under_conflict"),
            ("value constant mistaken for absent class", r"ConstResolution::Missing => \{\}", "ConstResolution::Missing => return false,", "alias_cycles_are_inconclusive_without_hiding_known_values_or_classes"),
            ("alias cycle mistaken for absence", r"if walk\.active\.contains\(&alias\) \{\s*return ConstResolution::Ambiguous;", "if walk.active.contains(&alias) { return ConstResolution::Missing;", "alias_cycles_are_inconclusive_without_hiding_known_values_or_classes"),
            ("alias hop limit mistaken for absence", r"if walk\.remaining == 0 \{\s*return ConstResolution::Ambiguous;", "if walk.remaining == 0 { return ConstResolution::Missing;", "alias_hop_limit_is_inconclusive_and_does_not_leak_between_queries"),
            ("alias budget not consumed", r"walk\.remaining -= 1;", "walk.remaining -= 0;", ("alias_hop_limit_is_inconclusive_and_does_not_leak_between_queries", "alias_budget_is_shared_across_qualified_segments")),
            ("alias budget reset per segment", r"found = self\.resolve_const_segment\(&\[\], &qualified, Some\(owner\), walk\);", "walk.remaining = Self::CONST_ALIAS_CHAIN_CAP;\n            found = self.resolve_const_segment(&[], &qualified, Some(owner), walk);", "alias_budget_is_shared_across_qualified_segments"),
            ("dynamic value does not shadow the lexical alias search", r"// alias: stop, exactly as `resolve_const`'s own shadow rule does\.\s*if single_segment \{", "// alias: stop, exactly as `resolve_const`'s own shadow rule does.\n            if false {", "a_dynamic_lexical_value_shadows_an_outer_alias_instead_of_resurrecting_it"),
            # The harvest used to trim the RHS cbase marker, which turned an
            # ABSOLUTE `X = ::Target` into a relative one re-evaluated in the
            # write scope - so a conflicting enclosing class made a top-level
            # target come back Ambiguous and silently dropped the arity check.
            ("alias RHS loses its cbase marker", r"join_path\(scope, &name\),\s*nesting\.to_vec\(\),\s*target,", "join_path(scope, &name),\n                        nesting.to_vec(),\n                        target.trim_start_matches(\"::\").to_string(),", "proven_lexical_alias_keeps_arity_and_navigation_under_conflict"),
        ]
        for label, pattern, after, expected in mutations:
            if len(re.findall(pattern, source)) != 1:
                raise SystemExit(f"INVALID-anchor {label}: expected exactly one source match")
            mutant = re.sub(pattern, lambda _: after, source, count=1)
            result, evidence = exercise(copy, original, mutant, expected)
            require(result, "CAUGHT", label)
            require(verdict(*evidence, "this_test_does_not_exist"), "WRONG-check", f"named-check guard: {label}")

        result, _ = exercise(copy, original, source, None)
        require(result, "PASS", "restored positive control")
    print("PASS: ancestry mutants and all four evidence guards")


if __name__ == "__main__":
    main()
