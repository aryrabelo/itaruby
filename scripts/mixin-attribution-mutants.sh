#!/usr/bin/env bash
# Two-sided proof for the attributed-mixin family (bead ita-a8z, phase A):
# the three suppression mechanisms that close rails' 160-site
# `Rails::AppBuilder` E0101 cluster, and the RECEIVER-keyed gate that
# keeps them from silencing a corpus (AGENTS.md, binding: the probes
# proving each side live next to the instrument, never only narrated -
# this file is where the mutation matrix lives instead of in a commit
# message).
#
# Each mutant removes exactly ONE load-bearing decision and must be
# ACCUSED by a NAMED test in crates/itaruby_semantic/tests/. A mutant
# that compiles with the suite green is a missing control, not a safe
# change (AGENTS.md): add the fixture, then count the mutant.
#
# MUT-1a the mixed-in module's `method_missing`/`respond_to_missing?`
#        gate stops being consulted: every attributed edge opens its
#        receiver whether or not the module answers unknown names
#        -> ternary_without_method_missing_still_accuses must fail:
#        that control's module deliberately answers nothing, and the
#        fixture's two candidate classes then go quiet
# MUT-1b the literal-constant receiver arm stops reading
#        -> attributed_literal_receiver_is_silenced must fail: the
#        `MixAttrBuilder.include(M)` edge is attributed to nobody, the
#        class stays closed, and `mix_attr_run` accuses again
# MUT-2a a const-returning body that IS a ternary of constant paths
#        stops contributing candidates (only a plain constant path
#        counts): the Rails `get_builder_class` shape
#        -> attributed_ternary_receiver_is_silenced must fail
# MUT-2b only the FIRST arm of that ternary is attributed (`.take(1)`)
#        -> attributed_ternary_opens_every_candidate must fail: the
#        else-arm class is exactly the one MRI does not instantiate in
#        the fixture, so nothing else can name it
# MUT-2c the receiverless-project-call arm (`builder_class =
#        get_builder_class`) stops deferring to phase 2
#        -> attributed_ternary_receiver_is_silenced must fail
# MUT-3a the interpolated-`def` harvest stops being called from the
#        class-body `class_eval` arm
#        -> interpolated_eval_defs_are_harvested must fail: the nine
#        forwarding names are invisible to every per-`def`-node scan
# MUT-3b the harvested names are not filed on the module (the synthetic
#        MethodDef is built and dropped)
#        -> interpolated_eval_names_are_filed_on_the_module must fail
# MUT-3c the eval call's RECEIVER stops being read: the harvest files on
#        the enclosing fragment whatever class the body really runs in
#        -> foreign_class_eval_defs_are_not_filed_here must fail: the
#        enclosing class gains a method it never has (the diagnostic
#        side is masked — that class opens for the `class_eval` call
#        itself — which is why that control is an INDEX assertion)
# MUT-1c the instance-only filter on the mixin TRACK stops applying:
#        an `extend` edge opens its receiver, silencing INSTANCE lookups
#        on a class whose singleton alone was widened
#        -> singleton_track_stays_closed must fail
#
# Builds/tests go to $ROOT/target: measuring what another target-dir
# produced is the 2026-09-17 stale-binary defect (AGENTS.md), and on a
# machine with a global `build.target-dir` two worktrees otherwise
# resolve each other's rmeta (2026-09-17).
set -uo pipefail

cd "$(dirname "$0")/.."
ROOT=$PWD
IDX=crates/itaruby_semantic/src/index.rs
BAK_IDX=$ROOT/target/mixin-attribution-index.rs.orig
LOG=$ROOT/target/mixin-attribution-mutants-test.txt

fail=0
ok() { printf 'PASS %s\n' "$1"; }
bad() { printf 'FAIL %s\n' "$1"; fail=1; }

mkdir -p "$ROOT/target"
cp "$IDX" "$BAK_IDX"
restore() { cp "$BAK_IDX" "$IDX"; }
trap restore EXIT

# One test-binary run over every suite these mutants can be accused by.
compiles=0
failing=
# Sets `compiles` and `failing` in the CALLER's shell - never called in a
# command substitution, because a subshell's assignments die with it and
# "did not compile" would read as "green".
run_suite() {
  # `--no-fail-fast` is load-bearing, not hygiene: cargo stops after the
  # first failing test BINARY, so without it a mutant whose control lives
  # in a later suite reported "expected X to fail" while X had never run
  # (the defect operand-types-mutants.sh hit as M15 in round 6).
  CARGO_TARGET_DIR="$ROOT/target" cargo test -p itaruby_semantic --no-fail-fast \
    --test mixin_attribution --test dynamic_mixin_method --test dynamic_include \
    --test singleton_track --test singleton_lookup --test open_reason \
    --test class_body_block --test included_hook --test mock_singleton_surface \
    --test gem_reopen_lockfile --test walker_fp --test operand_types >"$LOG" 2>&1
  if grep -q 'could not compile' "$LOG"; then compiles=0; else compiles=1; fi
  failing=$(grep -oE '^test [a-z0-9_]+ \.\.\. FAILED' "$LOG" | awk '{print $2}' | sort -u)
}

# `sed` cannot express these anchors: several span lines and all of them
# must match EXACTLY once. A miss prints INVALIDO and fails the script
# rather than silently mutating nothing - a mutant that was never
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
    bad "$label INVALIDO - anchor not found, mutant never injected"
    restore
    return
  fi
  run_suite
  if (( ! compiles )); then
    bad "$label did not compile - a mutant that cannot run proves nothing"
  elif [[ -z $failing ]]; then
    bad "$label BLIND - compiles and the suite is green: the control is missing, not the change safe"
  elif grep -qx "$expect" <<<"$failing"; then
    ok "$label accused by $expect ($(tr '\n' ' ' <<<"$failing" | sed 's/ $//'))"
  else
    bad "$label expected $expect to fail, got: $(tr '\n' ' ' <<<"$failing")"
  fi
  restore
  cmp -s "$IDX" "$BAK_IDX" || bad "$label source NOT restored byte-identical"
}

echo '--- baseline (shipped source)'
run_suite
if (( ! compiles )); then echo 'ABORT: shipped source does not compile'; exit 1; fi
if [[ -n $failing ]]; then
  echo "ABORT: baseline is not green - failing: $(tr '\n' ' ' <<<"$failing")"
  exit 1
fi
ok 'baseline green (every accusing fixture fires, every silent fixture quiet)'

mutant MUT-1a "$IDX" \
  '        let Some(mid) = index.resolve_const(&edge.module_nesting, &edge.module) else { continue };
        let module = index.class(mid);
        if !module.methods.contains_key("method_missing")
            && !module.methods.contains_key("respond_to_missing?")
        {
            continue;
        }' \
  '        let Some(mid) = index.resolve_const(&edge.module_nesting, &edge.module) else { continue };
        let _ = index.class(mid);' \
  ternary_without_method_missing_still_accuses \
  'the method_missing gate drops: an attributed edge opens its receiver even when the module answers nothing'

mutant MUT-1b "$IDX" \
  '        if let Some(path) = const_path_str(node) {
            return vec![MixinReceiver::Path { path, nesting: self.nesting.clone() }];
        }
        if let Some(ternary) = ternary_const_paths(node) {' \
  '        if let Some(ternary) = ternary_const_paths(node) {' \
  attributed_literal_receiver_is_silenced \
  'a literal-constant receiver stops being read: the M1 edge is attributed to nobody'

mutant MUT-2a "$IDX" \
  '        let paths = ternary_const_paths(&only)
            .or_else(|| const_path_str(&only).map(|p| vec![p]))
            .unwrap_or_default();' \
  '        let paths = const_path_str(&only).map(|p| vec![p]).unwrap_or_default();' \
  attributed_ternary_receiver_is_silenced \
  'a const-returning ternary stops contributing candidates: the Rails get_builder_class shape'

mutant MUT-2b "$IDX" \
  '        for path in paths {
            self.const_returning_methods.push((name.clone(), path, self.nesting.clone()));
        }' \
  '        for path in paths.into_iter().take(1) {
            self.const_returning_methods.push((name.clone(), path, self.nesting.clone()));
        }' \
  attributed_ternary_opens_every_candidate \
  'only the first ternary arm is attributed: the else-arm class stays closed'

mutant MUT-2c "$IDX" \
  '        if let Some(call) = node.as_call_node() {
            if call.receiver().is_none()
                && call.arguments().is_none()
                && call.block().is_none()
                && !call.name().as_slice().is_empty()
            {
                return vec![MixinReceiver::Call {
                    method: String::from_utf8_lossy(call.name().as_slice()).into_owned(),
                }];
            }
        }' \
  '' \
  attributed_ternary_receiver_is_silenced \
  'a receiverless project call stops deferring to phase 2: builder_class holds nothing'

mutant MUT-3a "$IDX" \
  '                        self.harvest_interpolated_eval_defs(i, &call);
' \
  '' \
  interpolated_eval_defs_are_harvested \
  'the interpolated-def harvest is not called: the forwarding names stay invisible'

mutant MUT-3b "$IDX" \
  '                for name in top_level_def_names(&source) {
                    let mut md = MethodDef::synthetic(name, 0, template.span);
                    md.arity_unknown = true;
                    self.fragments[i].methods.push(md);
                }' \
  '                for name in top_level_def_names(&source) {
                    let md = MethodDef::synthetic(name, 0, template.span);
                    let _ = md;
                }' \
  interpolated_eval_names_are_filed_on_the_module \
  'the harvested names are built and dropped: the module carries none of them'

mutant MUT-3c "$IDX" \
  '            if let Some(target) = &template.explicit_target {
                if *target != self.fragments[i].path {
                    continue;
                }
            }
' \
  '            let _ = &template.explicit_target;
' \
  foreign_class_eval_defs_are_not_filed_here \
  'the eval receiver stops being read: the names are filed on the enclosing class'

mutant MUT-1c "$IDX" \
  '        match mixin_track(node.name().as_slice()) {
            Some(MixinTrack::Instance) => {}
            _ => return,
        }
' \
  '        let Some(_track) = mixin_track(node.name().as_slice()) else { return };
' \
  singleton_track_stays_closed \
  'the instance-only track filter drops: an extend edge opens its receiver'

echo '--- restore and prove the shipped source is byte-identical'
restore
cmp -s "$IDX" "$BAK_IDX" && ok 'index.rs restored byte-identical' || bad 'source NOT restored'
run_suite
if (( ! compiles )); then bad 'rebuild of the shipped source failed'
elif [[ -n $failing ]]; then bad "shipped source is not green: $(tr '\n' ' ' <<<"$failing")"
else ok 'shipped source green after every mutant'; fi
trap - EXIT
rm -f "$BAK_IDX"

if (( fail )); then echo 'RESULT: FAIL (mixin attribution mutants)'; exit 1; fi
echo 'RESULT: PASS (mixin attribution mutants)'
