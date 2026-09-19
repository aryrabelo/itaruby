#!/usr/bin/env bash
# Two-sided proof for bead C (ita-w2c, phase A/onda 2): the two shapes of
# "a call proven safe by a predicate" — `if respond_to?(:m)` in the branch
# where it held, and `x.is_a?(Class) && x < Base` where the left operand
# proves `x` is a class/module object.
#
# Each mutant removes exactly ONE load-bearing decision and must be
# ACCUSED by a NAMED test in crates/itaruby_semantic/tests/guard_narrowing.rs.
# A mutant that compiles with the suite green is a missing control, not a
# safe change (AGENTS.md): add the fixture, then count the mutant.
#
# M1  the `respond_to?` guard is never pushed for the then-branch
#    -> respond_to_true_branch_is_silent must fail: `setup` accuses again
# M2  the two-argument form (`respond_to?(:m, true)`) stops being accepted
#    (a literal boolean second argument is now rejected)
#    -> respond_to_second_argument_form_is_silent must fail
# M3  the suppression channel in the NotFound arm is gone entirely
#    -> respond_to_true_branch_is_silent must fail
# M4  the guard is consulted for ANY call of that name, receiver or not
#    -> respond_to_guard_does_not_cover_an_explicit_receiver must fail:
#       `self` answering the name says nothing about `Helper.new`
# M5  a receiverless call stops being a keyable expression (only local
#    variables get a key)
#    -> class_object_guard_on_a_receiverless_call_is_silent must fail
# M6  a local variable read stops being keyable (only calls get a key)
#    -> class_object_guard_on_a_local_is_silent must fail
# M7  `is_a?(Module)` stops being recognised as a class-object proof
#    -> class_object_guard_on_is_a_module_is_silent must fail
# M8  a project constant named `Class` stops shadowing the core one
#    -> class_object_guard_bails_on_a_shadowed_class_constant must fail:
#       the guard would fire on evidence that proves nothing
# M9  the `&&` never pushes the fact for its right operand
#    -> class_object_local_is_silent must fail
# M10 the narrowed fact is never consumed when typing a receiver
#    -> class_object_guard_on_a_receiverless_call_is_silent must fail
#
# Deliberately NOT mutated, with the reason measured rather than assumed:
# the `is_a?`-name check, the single-argument check and the exact
# `Class`/`Module` path text are all part of the same "which predicate
# counts as evidence" decision, and weakening any of them only ever
# silences a shape no fixture constructs (extra silence, invariant #1's
# safe direction). The refusing half of each — the guard fires on a shape
# it must not — is what M7 and M8 pin, one per axis.
#
# Builds/tests go to $ROOT/target: measuring what another target-dir
# produced is the 2026-09-17 stale-binary defect (AGENTS.md), and on a
# machine with a global `build.target-dir` two worktrees otherwise resolve
# each other's rmeta (2026-09-17).
set -uo pipefail

cd "$(dirname "$0")/.."
ROOT=$PWD
CHK=crates/itaruby_semantic/src/check.rs
BAK_CHK=$ROOT/target/guard-narrowing-check.rs.orig
LOG=$ROOT/target/guard-narrowing-mutants-test.txt

fail=0
ok()  { printf 'PASS %s\n' "$1"; }
bad() { printf 'FAIL %s\n' "$1"; fail=1; }

mkdir -p "$ROOT/target"
cp "$CHK" "$BAK_CHK"
restore() { cp "$BAK_CHK" "$CHK"; }
trap restore EXIT

compiles=0
failing=
# Sets `compiles`/`failing` in the CALLER's shell — never in a command
# substitution, whose assignments would die with the subshell and read as
# "green".
run_suite() {
  CARGO_TARGET_DIR="$ROOT/target" cargo test -p itaruby_semantic --no-fail-fast \
    --test guard_narrowing >"$LOG" 2>&1
  if grep -q 'could not compile' "$LOG"; then compiles=0; else compiles=1; fi
  failing=$(grep -oE '^test [a-z0-9_]+ \.\.\. FAILED' "$LOG" | awk '{print $2}' | sort -u)
}

# Several anchors span lines and every one must match EXACTLY once. A miss
# prints INVALIDO and fails the script rather than silently mutating
# nothing — a mutant that was never injected is not a passing mutant.
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
  cmp -s "$CHK" "$BAK_CHK" || bad "$label source NOT restored byte-identical"
}

echo '--- baseline (shipped source)'
run_suite
if (( ! compiles )); then echo 'ABORT: shipped source does not compile'; exit 1; fi
if [[ -n $failing ]]; then
  echo "ABORT: baseline is not green - failing: $(tr '\n' ' ' <<<"$failing")"
  exit 1
fi
ok 'baseline green (every accusing fixture fires, every silent fixture quiet)'

mutant M1 "$CHK" \
  '                // `defined?` guard beside it, and symmetric for
                // `unless` below.
                let respond_to_guard = respond_to_guard_name(&n.predicate());
                if let Some(m) = &respond_to_guard {
                    self.respond_to_guards.push(m.clone());
                }' \
  '                // `defined?` guard beside it, and symmetric for
                // `unless` below.
                let respond_to_guard: Option<String> = None;' \
  respond_to_true_branch_is_silent \
  'the then-branch guard is never pushed: `setup` is diagnosed again'

mutant M2 "$CHK" \
  '        if second.as_true_node().is_none() && second.as_false_node().is_none() {
            return None;
        }' \
  '        if second.as_true_node().is_none() || second.as_false_node().is_none() {
            return None;
        }' \
  respond_to_second_argument_form_is_silent \
  'a literal boolean second argument stops being accepted by the guard'

mutant M3 "$CHK" \
  '                    if call.receiver().is_none()
                        && self.respond_to_guards.iter().any(|g| g == &name)
                    {
                        self.tally_inconclusive(None);
                        self.note_unknown_origin(call, UnkOrigin::ProjectRet, None);
                        return Ty::Unknown;
                    }' \
  '                    // mutant: the suppression channel is gone' \
  respond_to_true_branch_is_silent \
  'the NotFound arm stops consulting the guard: the guarded call accuses'

mutant M4 "$CHK" \
  '                    if call.receiver().is_none()
                        && self.respond_to_guards.iter().any(|g| g == &name)' \
  '                    if self.respond_to_guards.iter().any(|g| g == &name)' \
  respond_to_guard_does_not_cover_an_explicit_receiver \
  'the guard stops requiring a receiverless call: another object rides it'

mutant M5 "$CHK" \
  '    let call = node.as_call_node()?;
    (call.receiver().is_none() && call.arguments().is_none() && call.block().is_none())
        .then(|| String::from_utf8_lossy(call.name().as_slice()).into_owned())' \
  '    let _not_a_key = node.as_call_node()?;
    None' \
  class_object_guard_on_a_receiverless_call_is_silent \
  'a receiverless call stops being keyable: `rack_app` never gets a fact'

mutant M6 "$CHK" \
  '    if let Some(local) = node.as_local_variable_read_node() {
        return Some(String::from_utf8_lossy(local.name().as_slice()).into_owned());
    }' \
  '    // mutant: local reads get no key' \
  class_object_guard_on_a_local_is_silent \
  'a local read stops being keyable: `endpoint` never gets a fact'

mutant M7 "$CHK" \
  'if !matches!(core, "Class" | "Module") {' \
  'if !matches!(core, "Class") {' \
  class_object_guard_on_is_a_module_is_silent \
  '`is_a?(Module)` stops proving a class object (every class is one)'

mutant M8 "$CHK" \
  '        if let Some(id) = self.index.resolve_const(scope, &path) {
            if self.index.class(id).path.trim_start_matches("::") != core {
                return None;
            }
        }' \
  '        // mutant: a project constant named `Class` no longer bails' \
  class_object_guard_bails_on_a_shadowed_class_constant \
  'a shadowing `Class` constant licenses silence it must not'

mutant M9 "$CHK" \
  '                let guarded = self.class_object_guard(&n.left(), scope);
                if let Some(name) = &guarded {
                    self.narrowed_names.push((name.clone(), Ty::Unknown));
                }' \
  '                let guarded: Option<String> = None;' \
  class_object_guard_on_a_local_is_silent \
  'the `&&` never records the fact for its right operand'

mutant M10 "$CHK" \
  '        let recv_ty = call
            .receiver()
            .and_then(|r| self.narrowed_expr_ty(&r))
            .unwrap_or(recv_ty);' \
  '        let recv_ty = recv_ty;' \
  class_object_guard_on_a_receiverless_call_is_silent \
  'the narrowed fact is computed but never consumed when typing a receiver'

echo '--- restore and prove the shipped source is byte-identical'
restore
cmp -s "$CHK" "$BAK_CHK" && ok 'check.rs restored byte-identical' || bad 'source NOT restored'
run_suite
if (( ! compiles )); then bad 'rebuild of the shipped source failed'
elif [[ -n $failing ]]; then bad "shipped source is not green: $(tr '\n' ' ' <<<"$failing")"
else ok 'shipped source green after every mutant'; fi
trap - EXIT
rm -f "$BAK_CHK"

if (( fail )); then echo 'RESULT: FAIL (guard narrowing mutants)'; exit 1; fi
echo 'RESULT: PASS (guard narrowing mutants - 10 mutants, each accused by a named control)'