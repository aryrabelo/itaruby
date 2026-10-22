#!/usr/bin/env bash
# Two-sided proof for W11 (Tapioca open world): core-receiver E0101 in a
# Bundler project, licensed only by complete evidence of what every gem
# adds to the receiver's method chain. AGENTS.md, binding: every
# fail-closed condition is a silent fixture in
# crates/itaruby/tests/open_world_cli.rs, and each mutant below removes
# exactly ONE of those decisions and must be ACCUSED by the named fixture
# (it re-emits the E0101 the accusing fixtures pin). A mutant that
# compiles with the suite green is a missing control, not a safe change.
#
#   O1..O3   trap 1, declared type = exact class (finality, RBI and
#            project type-test overrides)
#   O4..O9   trap 2, complete evidence (missing/stale RBI, unaudited
#            Tapioca-ignored version, PATH gem, unloaded files)
#   O10..O16 trap 3, the whole chain (ancestor list, BasicObject,
#            include/prepend, method_missing, unreadable bodies, the
#            readable core reopening, a refinement)
#   O17..O18 trap 4, stdlib and audited gems
#   O19..O22 the opt-in and the two sibling codes (E0108 and E0107)
#   O23..O25 the shapes Tapioca really writes (unparsed annotation sigs,
#            restated core inclusions, and the limit of that allowance)
#   O26..O39 installed gem source (Tapioca drops aliases), per-class fail
#            closed, gap gems, top-level includes, literal core definers
#
# Builds/tests go to $ROOT/target (the 2026-09-17 stale-binary defect).
set -uo pipefail

cd "$(dirname "$0")/.."
ROOT=$PWD
CHK=crates/itaruby_semantic/src/check.rs
IDX=crates/itaruby_semantic/src/index.rs
OW=crates/itaruby_semantic/src/open_world.rs
MAIN=crates/itaruby/src/main.rs
FILES=("$CHK" "$IDX" "$OW" "$MAIN")
BAK=$ROOT/target/open-world-mutants-orig
LOG=$ROOT/target/open-world-mutants-test.txt

fail=0
ok()  { printf 'PASS %s\n' "$1"; }
bad() { printf 'FAIL %s\n' "$1"; fail=1; }

mkdir -p "$BAK"
for f in "${FILES[@]}"; do cp "$f" "$BAK/$(basename "$f")"; done
restore() { for f in "${FILES[@]}"; do cp "$BAK/$(basename "$f")" "$f"; done; }
trap restore EXIT

compiles=0
failing=
# Sets `compiles`/`failing` in the caller's shell (never in `$(...)`).
# `--no-fail-fast`: the control may live in the second test binary.
run_suite() {
  CARGO_TARGET_DIR="$ROOT/target" cargo test -p itaruby --no-fail-fast \
    --test open_world_cli --test tapioca_closed_world_cli >"$LOG" 2>&1
  if grep -q 'could not compile' "$LOG"; then compiles=0; else compiles=1; fi
  failing=$(grep -oE '^test [a-z0-9_]+ \.\.\. FAILED' "$LOG" | awk '{print $2}' | sort -u)
}

inject() {
  NEEDLE=$2 REPL=$3 python3 - "$1" <<'PY'
import os, sys
path = sys.argv[1]
src = open(path).read()
needle, repl = os.environ['NEEDLE'], os.environ['REPL']
n = src.count(needle)
if n != 1:
    print(f'INVALIDO: anchor matched {n} times in {path}', file=sys.stderr)
    sys.exit(2)
open(path, 'w').write(src.replace(needle, repl))
PY
}

# mutant <label> <file> <needle> <replacement> <must-fail test> <rationale>
mutant() {
  local label=$1 file=$2 needle=$3 repl=$4 expect=$5 why=$6
  printf '\n--- %s: %s\n' "$label" "$why"
  restore
  if ! inject "$file" "$needle" "$repl"; then
    bad "$label INVALIDO — anchor not found, mutant never injected"
    restore
    return
  fi
  run_suite
  if (( ! compiles )); then
    bad "$label did not compile — a mutant that cannot run proves nothing"
  elif [[ -z $failing ]]; then
    bad "$label BLIND — compiles and the suite is green: the control is missing, not the change safe"
  elif grep -qx "$expect" <<<"$failing"; then
    # A silent-side mutant must make the checker SPEAK (the code shows in
    # the failure text); a `positive side` mutant silences an accusing
    # fixture instead.
    if [[ $why == 'positive side'* ]] || grep -qE '(error|warning)\[E010[178]\]' "$LOG"; then
      ok "$label accused by $expect ($(tr '\n' ' ' <<<"$failing" | sed 's/ $//'))"
    else
      bad "$label $expect failed without re-emitting the diagnostic"
    fi
  else
    bad "$label expected $expect to fail, got: $(tr '\n' ' ' <<<"$failing")"
  fi
  restore
  for f in "${FILES[@]}"; do
    cmp -s "$f" "$BAK/$(basename "$f")" || bad "$label $f NOT restored byte-identical"
  done
}

echo '--- baseline (shipped source)'
run_suite
if (( ! compiles )); then echo 'ABORT: shipped source does not compile'; exit 1; fi
if [[ -n $failing ]]; then
  echo "ABORT: baseline is not green — failing: $(tr '\n' ' ' <<<"$failing")"
  exit 1
fi
ok 'baseline green (both accusing fixtures fire, every silent fixture quiet)'

# ---- trap 1: declared type = exact class ------------------------------

mutant O1 "$CHK" \
  '        !final_class
            || crate::index::rbi_type_test_hazard(self.db)
            || self.project_type_test_hazard()' \
  '        crate::index::rbi_type_test_hazard(self.db)
            || self.project_type_test_hazard()' \
  declared_non_final_type_stays_silent \
  'finality: `returns(String)` admits a subclass that defines the name'

mutant O2 "$CHK" \
  '        !final_class
            || crate::index::rbi_type_test_hazard(self.db)
            || self.project_type_test_hazard()' \
  '        !final_class
            || self.project_type_test_hazard()' \
  rbi_type_test_override_stays_silent \
  'a gem class whose `is_a?` answers Integer passes the sig'

mutant O3 "$CHK" \
  '        !final_class
            || crate::index::rbi_type_test_hazard(self.db)
            || self.project_type_test_hazard()' \
  '        !final_class
            || crate::index::rbi_type_test_hazard(self.db)' \
  project_type_test_override_stays_silent \
  'a project class whose `kind_of?` answers Integer passes the sig'

# ---- trap 2: complete evidence or nothing ------------------------------

mutant O4 "$OW" \
  '        let dir = installed_gem_dir(env.gem_roots, name, version).ok_or_else(|| {
            format!("gem `{name} ({version})` has no RBI and no installed source for its locked version")
        })?;' \
  '        let Some(dir) = installed_gem_dir(env.gem_roots, name, version) else { return Ok(()); };' \
  gem_without_rbi_stays_silent \
  'a locked gem without an RBI may reopen Integer invisibly'

mutant O5 "$OW" \
  '            v.strip_prefix(version)
                .is_some_and(|tail| tail.is_empty() || tail.starts_with('"'"'-'"'"'))' \
  '            !v.is_empty() || version.is_empty()' \
  stale_rbi_version_stays_silent \
  'an RBI for another version describes another gem'

mutant O6 "$OW" \
  '        .find(|(gem, versions, _)| *gem == name && versions.contains(&version))' \
  '        .find(|(gem, _, _)| *gem == name)' \
  unaudited_tapioca_ignored_version_stays_silent \
  'only an audited version of a Tapioca-ignored gem counts'

mutant O7 "$OW" \
  '            if !inside(lock_dir, &lock_dir.join(path)) {' \
  '            if false && !inside(lock_dir, &lock_dir.join(path)) {' \
  path_gem_outside_the_project_stays_silent \
  'a PATH gem outside the project is source nobody reads'

mutant O8 "$MAIN" \
  '    evidence.harvest.merge(unloaded);' \
  '' \
  unloaded_script_reopening_core_stays_silent \
  'an unloaded script (outside every `--ignore`) still runs and may reopen Integer'

mutant O9 "$CHK" \
  '            || evidence.harvest(self.db).type_test' \
  '' \
  ignored_path_type_test_override_stays_silent \
  'an unloaded type-test override still lets a fake pass the sig'

# ---- trap 3: the whole ancestor chain ----------------------------------

mutant O10 "$CHK" \
  '            || self.open_world_chain_answers(core_pollution_names(cc), &[name])' \
  '            || self.open_world_chain_answers(&[], &[name])' \
  rbi_module_included_into_comparable_stays_silent \
  'every ancestor namespace of the receiver, not only BasicObject'

mutant O11 "$CHK" \
  '        chain.iter().copied().chain(["BasicObject"]).any(|ns| {' \
  '        chain.iter().copied().any(|ns| {' \
  rbi_reopening_basic_object_stays_silent \
  '`BasicObject` sits on every chain and is not a core namespace'

mutant O12 "$IDX" \
  '    frag.includes
        .iter()
        .filter(|module| !crate::open_world::is_core_include(owner, module.trim_start_matches("::")))
        .chain(&frag.prepends)' \
  '    frag.prepends
        .iter()' \
  rbi_module_included_into_comparable_stays_silent \
  'a module included into an ancestor carries its names'

mutant O13 "$IDX" \
  '    frag.includes
        .iter()
        .filter(|module| !crate::open_world::is_core_include(owner, module.trim_start_matches("::")))
        .chain(&frag.prepends)' \
  '    frag.includes
        .iter()
        .filter(|module| !crate::open_world::is_core_include(owner, module.trim_start_matches("::")))' \
  rbi_module_prepended_to_kernel_stays_silent \
  'a module prepended to an ancestor carries its names'

mutant O14 "$IDX" \
  '                    if name == "method_missing" || name == "respond_to_missing?" {
                        self.open_class(i, OpenReason::MethodMissing);
                    }
                    // Singleton-track step N+1, shape (2)' \
  '                    // Singleton-track step N+1, shape (2)' \
  rbi_method_missing_on_object_stays_silent \
  '`method_missing` on an ancestor answers every name'

mutant O15 "$IDX" \
  '    if frag.body_unreadable {' \
  '    if false {' \
  unreadable_rbi_reopening_stays_silent \
  'a reopening body read only in part answers every name'

mutant O16 "$IDX" \
  '    if frag.body_unreadable {' \
  '    if frag.open {' \
  full_coverage_closes_the_world_and_rbi_declared_methods_stay_silent \
  'positive side: a plain core reopening is readable, or every RBI that reopens String would mute it'

mutant O17 "$CHK" \
  '                || self.index.refined_core.contains(*n)' \
  '' \
  project_refinement_of_another_name_stays_silent \
  'a project refinement of Integer may define the name'

# ---- trap 4: stdlib and audited gems -----------------------------------

mutant O18 "$CHK" \
  '            || core_inventory_names(cc).iter().any(|class| crate::open_world::stdlib_core_ext_has(class, name))' \
  '' \
  stdlib_extension_stays_silent \
  '`require "openssl"` gives Integer `to_bn`, and openssl is in no lockfile'

mutant O19 "$CHK" \
  '        if evidence.curated_names(self.db).contains(name)
            ||' \
  '        if' \
  curated_gem_name_stays_silent \
  'a Tapioca-ignored gem has no RBI; its audited names stand in for one'

# ---- the opt-in and the sibling codes -----------------------------------

mutant O20 "$MAIN" \
  '                return;
            }
        }
    }' \
  '                continue;
            }
        }
    }' \
  gem_without_rbi_stays_silent \
  'one gap anywhere keeps the whole run open'

mutant O21 "$CHK" \
  '            && !self.open_world_ops_polluted(recv_cc, arg_cc, op)' \
  '' \
  rbi_coercion_hook_keeps_e0108_silent \
  'E0108: a gem RBI coercion hook makes `1 + "x"` legal'

mutant O22 "$CHK" \
  '        if self.silent || !self.closed_world() || crate::TapiocaEvidence::try_get(self.db).is_some() {' \
  '        if self.silent || !self.closed_world() {' \
  gem_class_answering_both_names_keeps_e0107_off \
  'E0107 reads project and core candidates only; a gem object may answer both names'

# ---- the shapes Tapioca really writes ------------------------------------

mutant O23 "$IDX" \
  '                            self.fragments[i].body_unreadable = before;' \
  '' \
  tapioca_core_reopening_shape_stays_readable \
  'positive side: an unparsed annotation `sig` defines nothing; reading it as opaque mutes every RBI reopening'

mutant O24 "$IDX" \
  '        .filter(|module| !crate::open_world::is_core_include(owner, module.trim_start_matches("::")))' \
  '' \
  tapioca_core_reopening_shape_stays_readable \
  'positive side: Tapioca restates `include ::Comparable` in every String reopening; it is a core fact'

mutant O25 "$OW" \
  '            | ("Array" | "Hash", "Enumerable")
    )' \
  '            | ("Array" | "Hash", "Enumerable")
            | (_, _)
    )' \
  non_core_inclusion_of_a_chain_module_stays_silent \
  'only MRI'"'"'s own inclusions are free: `class Integer; include Enumerable` hands Integer `map`'

# ---- installed gem source and name-keyed core definers -----------------

mutant O26 "$OW" \
  '                Some(dir) => evidence.harvest.merge(harvest_gem(&dir, Reach::Targeted)),' \
  '                Some(_) => {}' \
  source_alias_the_rbi_omits_stays_silent \
  'Tapioca drops `alias_method` in `class Hash` (json_expressions 0.9.0): only the installed source names it'

mutant O27 "$OW" \
  '                None => evidence.harvest.open.extend(touched),' \
  '                None => {}' \
  missing_source_opens_the_class_its_rbi_reopens \
  'with no source on disk, the class that gem'"'"'s RBI reopens stays open'

mutant O28 "$OW" \
  '                None => evidence.harvest.open.extend(touched),' \
  '                None => evidence.harvest.open_all(),' \
  missing_source_for_a_hash_gem_leaves_integer_closed \
  'positive side: a gem that only reopens Hash cannot give Integer a method'

mutant O29 "$OW" \
  '        evidence.harvest.merge(harvest_gem(&dir, Reach::Everything));' \
  '' \
  gap_gem_source_defining_the_name_stays_silent \
  'a gap gem covered by its source still adds what that source defines'

mutant O30 "$OW" \
  '        if native_extension(&dir) {' \
  '        if false {' \
  gap_gem_with_native_extension_stays_silent \
  'C code defines methods no walker reads'

mutant O31 "$OW" \
  '    if defs.core_mixin {' \
  '    if true {' \
  gap_gem_with_clean_installed_source_is_covered \
  'positive side: a clean installed source covers a gap gem'

mutant O32 "$OW" \
  '        PollutionSource::Module(m) => module_names(modules, m, 0).map(|s| s.into_iter().collect()),' \
  '        PollutionSource::Module(_) => Some(Vec::new()),' \
  unloaded_toplevel_include_of_a_readable_module_stays_silent \
  'a top-level `include M` hands every object M'"'"'s methods'

mutant O33 "$OW" \
  '        None => STDLIB_MODULES
            .get(key)' \
  '        None => STDLIB_MODULES
            .get("")' \
  unloaded_toplevel_include_of_fileutils_still_accuses \
  'positive side: FileUtils'"'"'s public surface is empty, so `include FileUtils` adds nothing'

mutant O34 "$IDX" \
  '        Some(m) => vec![PollutionSource::Names(vec![m])],
        None => vec![PollutionSource::Opaque],
    };' \
  '        Some(m) => vec![PollutionSource::Names(vec![m])],
        None => Vec::new(),
    };' \
  dynamic_kernel_define_method_stays_silent \
  'a computed `define_method` name may be any name: the class it names is opaque (the blanket core_mixin mark for core injections, which no mutant could distinguish, was deleted)'

mutant O35 "$CHK" \
  '                        && self.core_names_unpolluted(cc, &name)' \
  '' \
  literal_kernel_define_method_answers_its_name \
  '`Kernel.define_method(:frobnicate)` gives every object that name'

mutant O36 "$CHK" \
  '        self.names_unpolluted(&chain, &[name, "method_missing", "respond_to_missing?"])' \
  '        false' \
  literal_kernel_define_method_leaves_other_names_closed \
  'positive side: a literal core definer stands down its own name only'

mutant O37 "$CHK" \
  '            || Self::harvest_answers(evidence.harvest(self.db), core_pollution_names(cc), &[name])' \
  '' \
  source_alias_the_rbi_omits_stays_silent \
  'the harvested names must reach the E0101 lookup'

mutant O38 "$OW" \
  '                Some(dir) => evidence.harvest.merge(harvest_gem(&dir, Reach::Targeted)),' \
  '                Some(dir) => evidence.harvest.merge(harvest_gem(&dir, Reach::Everything)),' \
  rbi_gem_untargeted_definer_is_left_to_its_rbi \
  'positive side: a load-time definer through an unnamed receiver is in the RBI if it reached the chain'

mutant O39 "$OW" \
  '    let untargeted = reach == Reach::Everything;' \
  '    let untargeted = false;' \
  gap_gem_untargeted_definer_stays_silent \
  'a gap gem has no RBI: a definer through an unnamed receiver may land on Integer'

echo
if (( fail )); then echo 'open-world mutants: FAIL'; exit 1; fi
echo 'open-world mutants: every mutant accused, source restored byte-identical'
