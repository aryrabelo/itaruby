#!/usr/bin/env bash
# Two-sided proof for the class-object E0101 flip (2026-09-21) and for
# every mechanism the flip stands on. AGENTS.md, binding: the probes
# proving each side live next to the instrument, never only narrated —
# this file is where the mutation matrix lives instead of in a commit
# message.
#
# The flip turned a conclusive `MethodLookup::NotFound` on the
# class-object track into a diagnostic. `lookup_singleton` owns the
# open-ancestor guard; `lookup_singleton_rbi` only softens its verdict.
# Every mutant below removes exactly ONE load-bearing decision — either
# one of the mechanisms that emptied the public-corpus residue, or the
# flip's emission/lookup guard — and must be ACCUSED by a NAMED test.
# A green mutant needs a missing control or a structural redundancy
# proof, never a waived mutant (AGENTS.md).
#
#   CO-A  `extended_module_surface` reverts to reading only the extended
#         module's OWN method map (the pre-ita-xta shallow read)
#         -> extend_module_include_resolves_transitively must fail:
#            `extend Translation` reaches `model_name` only through
#            `Translation`'s own `include Naming`
#   CO-B  the OPEN-ancestor arm of that same walk is cut
#         -> extend_open_module_never_accuses must fail: an unreadable
#            module in the extended ancestry must make the surface
#            unknown, never empty
#   CO-C  `core_object_instance_surface` goes back to `Class`/`Module`
#         only (the pre-ita-obx chain)
#         -> core_ext_object_reopening_resolves_on_the_class_object_track
#            must fail: `class Object; def obx_in?` is on every class
#            object's dispatch
#   CO-D  the `include Singleton` softening is cut
#         -> singleton_mixin_instance_never_accuses must fail
#   CO-E  that softening loses its NAME gate and softens every name
#         -> singleton_mixin_typo_accuses_after_the_flip must fail: the
#            misspelling is not one of the three the mixin installs
#   CO-F  block-nested class/module definitions stop being registered
#         -> block_nested_class_registers_at_its_lexical_path must fail:
#            the name falls back to an unrelated top-level stub
#   CO-G  the registered fragment is born CLOSED instead of open
#         -> block_nested_class_registers_at_its_lexical_path must fail
#            on its openness assertion: a body this walk never read is
#            not a surface anyone may conclude from
#   CO-H  the `define_singleton_method` install arm of the extended hook
#         is cut
#         -> define_singleton_method_hook_lands_on_the_extenders_class_object
#            must fail: the name stops being filed and the call accuses
#   CO-I  the hook's base-ESCAPE opacity check is cut
#         -> hook_base_escaping_into_a_block_opens_the_extender must
#            fail: installs inside a nested block become invisible and
#            the extender reads as a closed surface
#   CO-J  the def-body bare `eval` arm is cut
#         -> def_body_eval_opens_the_enclosing_class must fail
#   CO-K  the rebindable-block guard goes back to receiverless-only
#         -> self_receiver_in_a_rebindable_block_is_never_accused must
#            fail: `self.x = v` inside a `define_method` block is blamed
#            on the lexically enclosing class object
#   CO-L  the `queue_classic` -> `QC` override entry is cut
#         -> queue_classic_reopen_needs_the_exception_table_and_stays_silent
#            must fail: the lock name and the constant share no letters,
#            so the key can never pair them
#   CO-M  `BigDecimal` drops out of the gem Kernel table
#         -> bundled_gem_kernel_conversion_function_stays_silent must
#            fail
#   CO-N  an `include` inside `class << self` goes back to the INSTANCE
#         track
#         -> sclass_include_arity_is_checked must fail: the method that
#            carried E0102 was the singleton-included one
#   CO-O  the string-source pass loses its BARE-STUB gate and opens every
#         same-named class
#         -> string_source_definition_never_opens_a_real_class must fail
#   CO-P  the string-source pass is not run at all
#         -> string_source_definition_opens_a_bare_stub must fail
#   CO-Q  THE FLIP: the class-object E0101 emission is cut
#         -> extend_typo_accuses_on_the_class_object_track must fail
#   CO-R  THE LOOKUP GATE: singleton lookup ignores an open ancestor
#         -> unrecognized_class_body_call_keeps_the_receiver_open must
#            fail its diagnostic assertion: Widget.load_nmae gets E0101.
#         The shipped test also requires E0101 for the same typo on a
#         closed receiver, so blanket silence cannot pass.
#
# CO-R's former emission-blocker mutation was BLIND: Widget's unknown
# class-body call sets `open`, so lookup_singleton returns Inconclusive
# before the NotFound emission arm. The second ancestry walk there added
# no protection. It is removed; CO-R now mutates the effective guard.
# This is not a census-only probe: the named test checks diagnostics
# before labels, and must fail because the mutant emits a false E0101.
#
# Builds/tests go to $ROOT/target: measuring what another target-dir
# produced is the 2026-09-17 stale-binary defect (AGENTS.md), and on a
# machine with a global `build.target-dir` two worktrees otherwise
# resolve each other's rmeta (2026-09-17).
set -uo pipefail

cd "$(dirname "$0")/.."
ROOT=$PWD
IDX=crates/itaruby_semantic/src/index.rs
DIS=crates/itaruby_semantic/src/discovery.rs
CHK=crates/itaruby_semantic/src/check.rs
COR=crates/itaruby_semantic/src/core.rs
BAK_IDX=$ROOT/target/flip-index.rs.orig
BAK_DIS=$ROOT/target/flip-discovery.rs.orig
BAK_CHK=$ROOT/target/flip-check.rs.orig
BAK_COR=$ROOT/target/flip-core.rs.orig
LOG=$ROOT/target/class-object-flip-mutants-test.txt

fail=0
ok()  { printf 'PASS %s\n' "$1"; }
bad() { printf 'FAIL %s\n' "$1"; fail=1; }

mkdir -p "$ROOT/target"
cp "$IDX" "$BAK_IDX"
cp "$DIS" "$BAK_DIS"
cp "$CHK" "$BAK_CHK"
cp "$COR" "$BAK_COR"
restore() {
  cp "$BAK_IDX" "$IDX"
  cp "$BAK_DIS" "$DIS"
  cp "$BAK_CHK" "$CHK"
  cp "$BAK_COR" "$COR"
}
trap restore EXIT

# One test-binary run over every suite these mutants can be accused by.
compiles=0
failing=
# Sets `compiles` and `failing` in the CALLER's shell — never called in a
# command substitution, because a subshell's assignments die with it and
# "did not compile" would read as "green".
run_suite() {
  # `--no-fail-fast` is load-bearing, not hygiene: cargo stops after the
  # first failing test BINARY, so without it a mutant whose control lives
  # in a later suite reports "expected X to fail" while X never ran.
  CARGO_TARGET_DIR="$ROOT/target" cargo test -p itaruby_semantic --no-fail-fast \
    --test singleton_track --test singleton_lookup --test dark_singleton \
    --test extended_hook --test rebindable_guard --test gem_reopen_lockfile \
    --test gem_kernel_dsl >"$LOG" 2>&1
  if grep -q 'could not compile' "$LOG"; then compiles=0; else compiles=1; fi
  failing=$(grep -oE '^test [a-z0-9_]+ \.\.\. FAILED' "$LOG" | awk '{print $2}' | sort -u)
}

# `sed` cannot express these anchors: several span lines and all of them
# must match EXACTLY once. A miss prints INVALIDO and fails the script
# rather than silently mutating nothing — a mutant that was never
# injected is not a passing mutant.
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
    ok "$label accused by $expect ($(tr '\n' ' ' <<<"$failing" | sed 's/ $//'))"
  else
    bad "$label expected $expect to fail, got: $(tr '\n' ' ' <<<"$failing")"
  fi
  restore
  cmp -s "$IDX" "$BAK_IDX" && cmp -s "$DIS" "$BAK_DIS" \
    && cmp -s "$CHK" "$BAK_CHK" && cmp -s "$COR" "$BAK_COR" \
    || bad "$label source NOT restored byte-identical"
}

echo '--- baseline (shipped source)'
run_suite
if (( ! compiles )); then echo 'ABORT: shipped source does not compile'; exit 1; fi
if [[ -n $failing ]]; then
  echo "ABORT: baseline is not green — failing: $(tr '\n' ' ' <<<"$failing")"
  exit 1
fi
ok 'baseline green (every accusing fixture fires, every silent fixture quiet)'

mutant CO-A "$IDX" \
  '            let (chain, complete) = self.ancestors(mid);
            for &a in &chain {
                let m = self.class(a);
                if m.open {
                    return Some(MethodLookup::Inconclusive);
                }
                if let Some(sig) = m.methods.get(name) {
                    return Some(MethodLookup::Found(sig, a));
                }
            }
            if !complete {
                return Some(MethodLookup::Inconclusive);
            }' \
  '            if let Some(sig) = self.class(mid).methods.get(name) {
                return Some(MethodLookup::Found(sig, mid));
            }' \
  extend_module_include_resolves_transitively \
  'the extend consult stops walking the extended module own ancestry'

mutant CO-B "$IDX" \
  '                if m.open {
                    return Some(MethodLookup::Inconclusive);
                }
                if let Some(sig) = m.methods.get(name) {' \
  '                if let Some(sig) = m.methods.get(name) {' \
  extend_open_module_never_accuses \
  'an OPEN ancestor of the extended module stops making the surface unreadable'

mutant CO-C "$IDX" \
  '            &["Module", "Object", "Kernel", "BasicObject"]
        } else {
            &["Class", "Module", "Object", "Kernel", "BasicObject"]
        };' \
  '            &["Module", "Class"]
        } else {
            &["Class"]
        };' \
  core_ext_object_reopening_resolves_on_the_class_object_track \
  'the class-object chain stops at Class/Module: Object/Kernel reopenings go unread'

mutant CO-D "$IDX" \
  '        if matches!(name, "instance" | "_load" | "clone") && self.includes_singleton_mixin(id) {
            return true;
        }' \
  '' \
  singleton_mixin_instance_never_accuses \
  'the include-Singleton softening is cut'

mutant CO-E "$IDX" \
  'matches!(name, "instance" | "_load" | "clone") && self.includes_singleton_mixin(id)' \
  'self.includes_singleton_mixin(id)' \
  singleton_mixin_typo_accuses_after_the_flip \
  'the Singleton softening loses its NAME gate and swallows the typo too'

mutant CO-F "$IDX" \
  '                            self.harvest_block_nested_definitions(scope, nesting, &block);' \
  '' \
  block_nested_class_registers_at_its_lexical_path \
  'block-nested class/module definitions stop being registered'

mutant CO-G "$IDX" \
  '            frag.open = true;
            frag.open_reason = Some(OpenReason::BlockNestedDefinition);' \
  '' \
  block_nested_class_registers_at_its_lexical_path \
  'the registered fragment is born CLOSED: a body nobody read becomes a surface'

mutant CO-H "$IDX" \
  '            "define_singleton_method" => self.harvest_hook_define(i, call, true),' \
  '' \
  define_singleton_method_hook_lands_on_the_extenders_class_object \
  'the singleton spelling of a hook install stops being filed'

mutant CO-I "$IDX" \
  '        if count_local_reads(&body, &pname) > consumed {
            self.fragments[i].hook_installs_opaque = true;
        }' \
  '' \
  hook_base_escaping_into_a_block_opens_the_extender \
  'the hook base-escape opacity check is cut: nested-block installs read as absent'

mutant CO-J "$IDX" \
  '        b"eval" => (!args.is_empty()).then_some(OpenReason::EvalOrSend),' \
  '' \
  def_body_eval_opens_the_enclosing_class \
  'a bare eval in a method body stops opening its class'

mutant CO-K "$CHK" \
  '            && call.receiver().is_none_or(|r| r.as_self_node().is_some())' \
  '            && call.receiver().is_none()' \
  self_receiver_in_a_rebindable_block_is_never_accused \
  'the rebindable guard goes back to receiverless-only: explicit self is blamed on the lexical class'

mutant CO-L "$DIS" \
  '        "queue_classic" => return Some("QC".to_string()),' \
  '' \
  queue_classic_reopen_needs_the_exception_table_and_stays_silent \
  'the queue_classic -> QC override entry is cut'

mutant CO-M "$COR" \
  'const GEM_KERNEL_METHODS: &[&str] = &["Stoplight", "Rainbow", "_", "s_", "BigDecimal"];' \
  'const GEM_KERNEL_METHODS: &[&str] = &["Stoplight", "Rainbow", "_", "s_"];' \
  bundled_gem_kernel_conversion_function_stays_silent \
  'BigDecimal drops out of the bundled-gem Kernel table'

mutant CO-N "$IDX" \
  '                                    "include" | "prepend" if in_singleton => {
                                        if !self.fragments[i].extends.contains(&path) {
                                            self.fragments[i].extends.push(path);
                                        }
                                    }' \
  '' \
  sclass_include_arity_is_checked \
  'an include inside class << self goes back to the INSTANCE track'

mutant CO-O "$IDX" \
  '        if !class.methods.is_empty() || !class.singleton_methods.is_empty() {
            continue;
        }' \
  '' \
  string_source_definition_never_opens_a_real_class \
  'the string-source pass loses its bare-stub gate and blinds real classes'

mutant CO-P "$IDX" \
  '    apply_string_source_definitions(&mut index);' \
  '' \
  string_source_definition_opens_a_bare_stub \
  'the string-source pass never runs'

mutant CO-Q "$CHK" \
  '                        self.emit_with(
                            msg_loc.0,
                            msg_loc.1,
                            E0101_UNKNOWN_METHOD,
                            Severity::Error,
                            message,
                            suggestion,
                        );
                        Ty::Unknown' \
  '                        let _ = (message, suggestion);
                        Ty::Unknown' \
  extend_typo_accuses_on_the_class_object_track \
  'THE FLIP: the class-object E0101 emission is cut'

mutant CO-R "$IDX" \
  '            if class.open {
                return MethodLookup::Inconclusive;
            }
            if let Some(m) = class.singleton_methods.get(name) {' \
  '            if let Some(m) = class.singleton_methods.get(name) {' \
  unrecognized_class_body_call_keeps_the_receiver_open \
  'THE LOOKUP GATE: an open singleton ancestor is treated as closed and emits E0101'

echo
if (( fail )); then
  echo 'RESULT: FAIL (a mutant was blind, unaccused, or the source was not restored)'
  exit 1
fi
echo 'RESULT: PASS (every mutant accused by its named test, source restored byte-identical)'
