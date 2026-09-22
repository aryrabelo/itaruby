#!/usr/bin/env python3
"""Two-sided Sorbet sig/RBI contract probes. Builds only a temporary source copy, never the checkout.

Run: python3 scripts/sorbet-contracts-mutants.py            (full family)
     python3 scripts/sorbet-contracts-mutants.py --anchors  (count every needle, build nothing)

Each mutant removes ONE contract decision and must be accused by a NAMED test.
Needles are literal and counted with `src.count(needle)` against the source
BEFORE anything builds, so a dead anchor is an INVALID-anchor in milliseconds
instead of a blind mutant half an hour in. The baseline and each mutant run the
contract suites; invalid/no-op mutations, compiler failures, and failures of
the wrong test are distinct verdicts.
"""
from pathlib import Path
import filecmp
import os
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
SRC = Path("crates/itaruby_semantic/src")
CHECK = SRC / "check.rs"
INDEX = SRC / "index.rs"
SIG = SRC / "sorbet_sig.rs"
TESTS = ("sorbet_contracts", "sorbet_contract_parser", "project_sigs", "sorbet_sig")

# (label, file, needle, replacement, named test(s) that must FAIL)
MUTATIONS = [
    # -- eligibility: which definition a written signature is proven to govern
    ("open owner governs its sig", CHECK,
     "        if class.open {\n            return None;\n        }\n        let same = ",
     "        let same = ",
     "open_source_keeps_existing_ruby_checks_without_contract_accusations"),
    ("descendant override ignored", CHECK,
     "        let diverges = self.index.contract_dispatch_diverges(owner, name, singleton, method.file, method.def_span);\n",
     "        let diverges = false;\n",
     "descendant_override_keeps_the_parent_contract_off_the_call"),
    ("duplicate source picks the last sig", INDEX,
     "        method.sorbet_sig = None;\n        method.sorbet_annotated = true;\n",
     "",
     "duplicate_source_definitions_do_not_pick_a_signature"),
    # -- RBI: exact owner, track and layout
    ("stale RBI layout accepted", CHECK,
     "            if !declaration.matches_source(method) {\n                return None;\n            }\n",
     "",
     "stale_rbi_layout_does_not_type_source_or_calls"),
    ("RBI positional names ignored", INDEX,
     "            && source.positional_names == md.positional_names\n",
     "",
     "stale_rbi_layout_does_not_type_source_or_calls"),
    ("conflicting RBI declarations pick one", INDEX,
     "                || old.nesting != declaration.nesting\n            {\n                old.definition.sorbet_sig = None;",
     "                || old.nesting != declaration.nesting\n            {\n                let _ = ();",
     "conflicting_rbi_declarations_do_not_choose_a_contract"),
    ("inherited RBI owner lends its contract", INDEX,
     "methods.get(method).filter(|m| m.owner == path).cloned()",
     "methods.get(method).cloned()",
     "inherited_rbi_contract_never_attaches_to_a_source_override"),
    # -- instance / singleton separation
    ("source track forced to instance", CHECK,
     "        let singleton = class.singleton_methods.get(name).is_some_and(same);",
     "        let singleton = false;",
     "rbi_singleton_contract_matches_source_track"),
    ("RBI track forced to instance", INDEX,
     "    let methods = if singleton { singleton_methods } else { instance };\n    methods.get(method).filter",
     "    let methods = instance;\n    methods.get(method).filter",
     "rbi_singleton_contract_matches_source_track"),
    # -- E0109: the body answers to its own signature
    ("body never checked against sig", CHECK,
     "            self.check_sorbet_return(def, contract, nesting, &last, &returns);",
     "            let _ = (contract, nesting);",
     ("incompatible_source_return_accuses_without_a_call",
      "explicit_returns_and_implicit_branches_are_checked")),
    ("explicit returns not inspected", CHECK,
     "        let actual = returns.iter().find(|ty| contract_accuses(ty, &expected, returns_nil, self.index))\n            .or_else(",
     "        let actual = None\n            .or_else(",
     "explicit_returns_and_implicit_branches_are_checked"),
    ("uncertain return paths accused", CHECK,
     "        if safety.uncertain {\n            return;\n        }\n",
     "",
     "void_unknown_and_uncertain_return_paths_stay_silent"),
    # `void` and `returns` share one clause bit, so a void sig never carries
    # `ret`: deleting the `contract.void` guard alone is an equivalent mutant
    # (measured BLIND). The observable failure of this decision is reading
    # `void` as a promise of `nil`, so the mutant drops the guard AND does that.
    ("void sig treated as a return type", CHECK,
     "        if self.silent || contract.void {\n            return;\n        }\n"
     "        let (Some(expr), Some(body)) = (contract.ret.as_deref(), def.body()) else { return };",
     "        if self.silent {\n            return;\n        }\n"
     "        let (Some(expr), Some(body)) = (contract.ret.as_deref()"
     ".or(contract.void.then_some(\"NilClass\")), def.body()) else { return };",
     "void_unknown_and_uncertain_return_paths_stay_silent"),
    # -- E0103: parameters correspond by NAME, never by position
    ("params zipped by sig order", CHECK,
     "        for (name, (ty, span, _)) in positional_names.iter().zip(args.positional) {",
     "        for (name, (ty, span, _)) in sig.params.iter().map(|(n, _)| n).zip(args.positional) {",
     "named_positional_params_accuse_at_argument"),
    ("keyword args never checked", CHECK,
     "        for (name, ty, span) in keyword_args {\n            let literal_nil",
     "        for (name, ty, span) in keyword_args.iter().take(0) {\n            let literal_nil",
     "keywords_and_defaults_keep_named_correspondence"),
    ("splat keeps positional correspondence", CHECK,
     "                        exact_arity = false;\n                        sorbet_args_known = false;\n                        self.infer_expr(&a, env, self_ty, scope);",
     "                        exact_arity = false;\n                        self.infer_expr(&a, env, self_ty, scope);",
     "keywords_and_defaults_keep_named_correspondence"),
    ("unmatched sig names guessed", INDEX,
     "            if !matches {\n                md.sorbet_sig = None;\n            }",
     "            if !matches {\n                let _ = ();\n            }",
     "sig_naming_an_absent_parameter_is_not_a_contract"),
    # -- PendingSig::Unusable: unreadable or stacked sigs still count as ANNOTATED
    ("stacked sigs pick the last overload", INDEX,
     "                            Some(parsed) if !stacked => PendingSig::Parsed(parsed),",
     "                            Some(parsed) => PendingSig::Parsed(parsed),",
     "overloads_and_unsupported_layouts_do_not_invent_correspondence"),
    ("unusable sig counts as unannotated", INDEX,
     "            sorbet_annotated: pending_sorbet_sig.is_some(),",
     "            sorbet_annotated: matches!(pending_sorbet_sig, Some(PendingSig::Parsed(_))),",
     "unusable_inline_sig_still_blocks_the_rbi_contract"),
    # -- nil is proven only where it is written (invariant #1)
    ("nil in a union counts as proven", CHECK,
     "            Ty::Union(parts) => Ty::Union(parts.iter().map(unprove_nil).collect()),",
     "            Ty::Union(parts) => Ty::Union(parts.clone()),",
     ("nilable_return_through_defaults_and_guards_stays_silent",
      "nilable_argument_through_defaults_and_guards_stays_silent")),
    ("nil read from a variable counts as proven", CHECK,
     "    let proven = if literal_nil && *actual == Ty::Nil { Ty::Nil }",
     "    let proven = if *actual == Ty::Nil { Ty::Nil }",
     "nil_read_from_a_variable_is_not_proof"),
    ("literal nil never accused", CHECK,
     "    let proven = if literal_nil && *actual == Ty::Nil { Ty::Nil }",
     "    let proven = if false && literal_nil && *actual == Ty::Nil { Ty::Nil }",
     "literal_nil_and_mismatched_union_members_still_accuse"),
    # -- sig names: a core name stays core, an inherited name never falls to the top level
    ("reopened core name becomes a project instance", SIG,
     "    if crate::core::is_known_core_constant(path) {\n        return Ty::Unknown;\n    }\n",
     "",
     "reopened_core_names_keep_their_core_meaning"),
    ("module mixed into core keeps its instance", SIG,
     "    if mixed_into_core(id, index) {\n        return Ty::Unknown;\n    }\n",
     "",
     "project_module_mixed_into_core_never_accuses_core_values"),
    ("ancestor namespace shadow ignored", INDEX,
     "                    return ConstFallback::Shadowed;\n",
     "                    let _ = ();\n",
     "inherited_namespace_constants_never_bind_to_top_level"),
    ("opaque ancestry proves the top-level fallback", SIG,
     "            ConstFallback::Opaque if matches!(ty, Ty::Instance(_)) => Ty::Unknown,\n",
     "",
     "inherited_namespace_constants_never_bind_to_top_level"),
    # -- core returns follow their arguments; unproven is Unknown, never a guess
    ("integer operand type ignored", CHECK,
     "            [Ty::Int] => Ty::Int,\n            [Ty::Float] => Ty::Float,\n            _ => Ty::Unknown,\n",
     "            _ => Ty::Int,\n",
     "integer_arithmetic_takes_the_operand_type"),
    ("float operand type ignored", CHECK,
     "            [Ty::Int | Ty::Float] => Ty::Float,\n            _ => Ty::Unknown,\n",
     "            _ => Ty::Float,\n",
     "integer_arithmetic_takes_the_operand_type"),
    ("float digits ignored", CHECK,
     "        ([Ty::Int], Some(n)) if n > 0 => Ty::Float,\n",
     "",
     "float_rounding_with_digits_is_not_an_integer"),
    ("array count returns one element", CHECK,
     "            ([Ty::Int], Ty::Array(_)) => Some(recv.clone()),\n",
     "            ([Ty::Int], Ty::Array(_)) => None,\n",
     "array_count_argument_returns_an_array"),
    ("hidden argument shape keeps the table return", CHECK,
     "        return if core_ret_depends_on_args(cc, name) { Ty::Unknown } else { core_ret_to_ty(ret, recv) };",
     "        return core_ret_to_ty(ret, recv);",
     "array_count_argument_returns_an_array"),
    ("flatten keeps the nesting", CHECK,
     "        \"flatten\" => Some(flatten_ret(recv, args.is_empty())),",
     "        \"flatten\" => None,",
     "flatten_drops_the_nesting"),
    ("clamp bounds ignored", CHECK,
     "        \"clamp\" => match args {\n            [Ty::Int, Ty::Int] => Ty::Int,\n            _ => Ty::Unknown,\n        },\n",
     "        \"clamp\" => Ty::Int,\n",
     "integer_clamp_with_float_bounds_is_unproven"),
    # -- an ivar is proven only when every writer path is visible
    ("attribute writer ignored", CHECK,
     "        if !matches!(self.index.lookup_method(class, &writer), MethodLookup::NotFound)\n"
     "            || self.index.descendant_defines(class, &writer, false)\n        {\n            return true;\n        }\n",
     "",
     "ivar_with_an_attribute_writer_is_unproven"),
    ("project-wide hidden writes ignored", CHECK,
     "        if self.index.hidden_ivar_writes_any || self.index.hidden_ivar_writes.contains(name) {\n"
     "            return true;\n        }\n",
     "",
     "ivar_written_by_reflection_is_unproven"),
    ("reflection names not hidden", INDEX,
     "            Some(name) => self.hide_ivar(&name),",
     "            Some(_) => {}",
     "ivar_written_by_reflection_is_unproven"),
    ("self-rebinding block writes attributed lexically", INDEX,
     "        self.ivar_scopes.push(if lexical && in_def { IvarScope::InstanceDef } else { IvarScope::Opaque });",
     "        self.ivar_scopes.push(IvarScope::InstanceDef);",
     "ivar_written_by_reflection_is_unproven"),
    ("family writes ignored", CHECK,
     "        family.into_iter().any(|member| {",
     "        family.into_iter().take(0).any(|member| {",
     "ivar_written_elsewhere_in_the_family_is_unproven"),
    ("or-write is not a write", CHECK,
     "                let n = node.as_instance_variable_or_write_node().unwrap();\n"
     "                self.infer_expr(&n.value(), env, self_ty, scope);\n"
     "                self.capture_ivar_write(self_ty, n.name().as_slice(), Ty::Unknown);\n",
     "                let n = node.as_instance_variable_or_write_node().unwrap();\n"
     "                self.infer_expr(&n.value(), env, self_ty, scope);\n",
     "ivar_compound_writes_in_the_same_class_are_unproven"),
    ("multiple-assignment target is not a write", INDEX,
     "        self.hide_ivar(node.name().as_slice());\n        ruby_prism::visit_instance_variable_target_node",
     "        ruby_prism::visit_instance_variable_target_node",
     "ivar_compound_writes_in_the_same_class_are_unproven"),
    # -- every redefinition path takes a written contract off the method
    ("singleton patch keeps the stale contract", INDEX,
     "                std::collections::hash_map::Entry::Occupied(mut slot) => poison_contract(slot.get_mut()),",
     "                std::collections::hash_map::Entry::Occupied(_) => {}",
     "singleton_patch_redefinition_takes_the_contract_off"),
    ("extended hook keeps the stale contract", INDEX,
     "            slot.insert(hook_install_sig(span, file));\n        }\n"
     "        std::collections::hash_map::Entry::Occupied(mut slot) => poison_contract(slot.get_mut()),",
     "            slot.insert(hook_install_sig(span, file));\n        }\n"
     "        std::collections::hash_map::Entry::Occupied(_) => {}",
     "extended_hook_redefinition_takes_the_contract_off"),
    ("outside redefinitions keep the stale contract", INDEX,
     "    poison_injected_contracts(&mut index);\n",
     "",
     "class_eval_redefinition_takes_the_contract_off"),
    ("include-time code keeps the stale contract", INDEX,
     "    poison_include_time_redefinitions(&mut index);\n",
     "",
     "included_hook_redefinition_takes_the_contract_off"),
    ("include hook methods ignored", INDEX,
     "    INCLUDE_HOOKS.iter().any(|hook| module.singleton_methods.contains_key(*hook))\n",
     "    INCLUDE_HOOKS.iter().take(0).any(|hook| module.singleton_methods.contains_key(*hook))\n",
     "included_hook_redefinition_takes_the_contract_off"),
    ("open project module trusted at include time", INDEX,
     "        || (module.open\n",
     "        || (false && module.open\n",
     "included_hook_redefinition_takes_the_contract_off"),
    ("descendant include-time code trusted", INDEX,
     "        if class.open || self.include_time_code.contains(&id) {",
     "        if class.open {",
     "include_time_code_in_a_descendant_keeps_the_parent_contract_off_the_call"),
    ("module family never walked", INDEX,
     "            let mixed = self.mixers.get(&cur).into_iter()",
     "            let mixed = None::<&Vec<Mixer>>.into_iter()",
     "module_contract_yields_to_an_includer_family_override"),
    ("mixing member trusted by its own map", INDEX,
     "        let mixes = !class.includes.is_empty()",
     "        let mixes = false && !class.includes.is_empty()",
     "descendant_mixin_override_keeps_the_parent_contract_off_the_call"),
    # -- cost: contract work is per definition, never per call
    ("contract memo bypassed", CHECK,
     "        if let Some(hit) = self.contract_memo.borrow().get(&key) {\n            return hit.clone();\n        }\n",
     "",
     "contract_work_is_per_definition_not_per_call"),
    ("family walked before the contract", CHECK,
     "        let (owner, name, singleton) = self.dispatching_name(method, name, path)?;\n",
     "        let (owner, name, singleton) = self.dispatching_name(method, name, path)?;\n"
     "        let _ = self.index.contract_dispatch_diverges(owner, name, singleton, method.file, method.def_span);\n",
     "contract_work_is_per_definition_not_per_call"),
    ("return memo read after sig_fill", CHECK,
     "        // A memo entry is only ever written after `sig_fill` answered\n",
     "        let _ = self.sig_fill(m, name);\n",
     "contract_work_is_per_definition_not_per_call"),
    # -- self in a module is an instance of an includer nobody named
    ("module instance proves incompatibility", CHECK,
     "        (Ty::Instance(a), _) if index.class(*a).is_module => true,\n",
     "",
     "module_self_is_never_proof_against_an_includer"),
    # -- collection type arguments are erased at runtime; only the category binds
    ("type arguments held as a contract", CHECK,
     "    !compatible(&erase_type_arguments(&proven), expected, index)",
     "    !compatible(&proven, expected, index)",
     "collection_type_arguments_are_erased_but_the_category_still_accuses"),
    # -- the keyword loop still WALKS what it cannot bind by name
    ("non-symbol keyword pair left unwalked", CHECK,
     "                                    self.infer_expr(&assoc.key(), env, self_ty, scope);\n                                    self.infer_expr(&assoc.value(), env, self_ty, scope);\n",
     "                                    self.infer_expr(&element, env, self_ty, scope);\n",
     "unbindable_keyword_elements_are_still_checked"),
    ("keyword splat left unwalked", CHECK,
     "                                    Some(value) => { self.infer_expr(&value, env, self_ty, scope); }",
     "                                    Some(_) => { self.infer_expr(&element, env, self_ty, scope); }",
     "unbindable_keyword_elements_are_still_checked"),
]


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


def exercise(copy, path, original, content, expected):
    target = copy / path
    target.write_text(content)
    try:
        if expected and filecmp.cmp(original, target, shallow=False):
            return "INVALID-cmp", None
        built = run(copy, "--no-run")
        if built.returncode != 0:
            return "INVALID-build", (built.returncode, None, built.stdout)
        tested = run(copy, "--", "--test-threads=1")
        evidence = (built.returncode, tested.returncode, tested.stdout)
        return verdict(*evidence, expected), evidence
    finally:
        # Every mutant starts from the shipped source, byte-identical.
        shutil.copyfile(original, target)
        if not filecmp.cmp(original, target, shallow=False):
            raise SystemExit(f"RESTORE-FAILED {path}")


def require(actual, expected, label):
    print(f"{label}: {actual}", flush=True)
    if actual != expected:
        raise SystemExit(f"{label}: expected {expected}, got {actual}")


def count_anchors(sources):
    broken = []
    for label, path, needle, _after, _expected in MUTATIONS:
        n = sources[path].count(needle)
        if n != 1:
            broken.append(f"INVALID-anchor {label}: needle matches {n} times in {path}")
    for line in broken:
        print(line)
    print(f"{len(MUTATIONS)} anchors checked in {len({m[1] for m in MUTATIONS})} files")
    return not broken


def main():
    sources = {path: (ROOT / path).read_text() for path in {m[1] for m in MUTATIONS}}
    if not count_anchors(sources):
        raise SystemExit(1)
    if "--anchors" in sys.argv[1:]:
        print("PASS every anchor matches exactly once")
        return
    with tempfile.TemporaryDirectory(prefix="ita-sorbet-mutants-") as temporary:
        copy = Path(temporary)
        for name in ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml"]:
            shutil.copy2(ROOT / name, copy / name)
        shutil.copytree(ROOT / "crates", copy / "crates", ignore=shutil.ignore_patterns("target"))
        # Cargo builds this semantic-crate bin for integration tests too.
        # Copy its public source explicitly, never the scripts directory
        # (which may contain machine-local corpus configuration).
        (copy / "scripts").mkdir()
        generator = Path("scripts/gen-activerecord-inventory.rs")
        shutil.copy2(ROOT / generator, copy / generator)
        originals = {}
        for path, text in sources.items():
            original = copy / f"original-{path.name}"
            original.write_text(text)
            originals[path] = original

        first = next(iter(sources))
        result, _ = exercise(copy, first, originals[first], sources[first], None)
        require(result, "PASS", "positive control")
        result, _ = exercise(copy, CHECK, originals[CHECK], sources[CHECK], MUTATIONS[0][4])
        require(result, "INVALID-cmp", "no-op guard")
        result, _ = exercise(copy, CHECK, originals[CHECK], sources[CHECK] + "\nnot valid Rust;\n", MUTATIONS[0][4])
        require(result, "INVALID-build", "compiler guard")

        for label, path, needle, after, expected in MUTATIONS:
            mutant = sources[path].replace(needle, after, 1)
            result, evidence = exercise(copy, path, originals[path], mutant, expected)
            require(result, "CAUGHT", label)
            require(verdict(*evidence, "this_test_does_not_exist"), "WRONG-check", f"named-check guard: {label}")

        result, _ = exercise(copy, first, originals[first], sources[first], None)
        require(result, "PASS", "restored positive control")
    print(f"PASS: {len(MUTATIONS)} sorbet contract mutants and all four evidence guards")


if __name__ == "__main__":
    main()
