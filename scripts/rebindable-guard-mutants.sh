#!/usr/bin/env bash
# Two-sided proof for bead F (ita-w2c, phase A/onda 2): the rebindable-
# block guard sits ABOVE the `lookup_method` dispatch, so the
# Found/arity/sig path consults it too — `body html` inside
# `Mail::Part.new { ... }` is not the enclosing builder's zero-argument
# `body`.
#
# Each mutant removes exactly ONE load-bearing decision and must be
# ACCUSED by a NAMED test in crates/itaruby_semantic/tests/rebindable_guard.rs.
# A mutant that compiles with the suite green is a missing control, not a
# safe change (AGENTS.md): add the fixture, then count the mutant.
#
# M1 the guard is not consulted above the dispatch at all
#    -> self_send_inside_a_rebindable_block_is_silent must fail (the
#       Found/arity path accuses the block's self-send again)
# M2 the guard stops requiring a RECEIVERLESS call
#    -> explicit_receiver_in_a_rebindable_block_still_accuses must fail:
#       `helper.w2c_absent_step` is `instance_eval`-proof and must stay
#       conclusive
# M3 no block is ever considered rebindable
#    -> self_send_inside_a_rebindable_block_is_silent must fail: the
#       silent fixture really does depend on the rebindable determination,
#       it is not quiet by accident
#
# Builds/tests go to $ROOT/target: measuring what another target-dir
# produced is the 2026-09-17 stale-binary defect (AGENTS.md), and on a
# machine with a global `build.target-dir` two worktrees otherwise resolve
# each other's rmeta (2026-09-17).
set -uo pipefail

cd "$(dirname "$0")/.."
ROOT=$PWD
CHK=crates/itaruby_semantic/src/check.rs
BAK_CHK=$ROOT/target/rebindable-guard-check.rs.orig
LOG=$ROOT/target/rebindable-guard-mutants-test.txt

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
    --test rebindable_guard >"$LOG" 2>&1
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
  '        if self.rebindable_block_depth > 0
            && call.receiver().is_none_or(|r| r.as_self_node().is_some())
        {
            self.tally_inconclusive(None);
            self.note_unknown_origin(call, UnkOrigin::ProjectRet, None);
            return Ty::Unknown;
        }' \
  '        // mutant: the guard no longer sits above the dispatch' \
  self_send_inside_a_rebindable_block_is_silent \
  'the Found/arity path stops consulting the guard: `body html` accuses again'

mutant M2 "$CHK" \
  '        if self.rebindable_block_depth > 0
            && call.receiver().is_none_or(|r| r.as_self_node().is_some())
        {' \
  '        if self.rebindable_block_depth > 0 {' \
  explicit_receiver_in_a_rebindable_block_still_accuses \
  'the receiver clause is dropped: every call in a rebindable block is silenced, so an explicit `Helper` receiver `instance_eval` cannot change is wrongly softened'

mutant M4 "$CHK" \
  '            && call.receiver().is_none_or(|r| r.as_self_node().is_some())' \
  '            && call.receiver().is_none()' \
  self_receiver_in_a_rebindable_block_is_never_accused \
  'the ita-slf self-arm is cut: an explicit `self` receiver in a rebindable block accuses again, though `self` IS the rebound object'

mutant M3 "$CHK" \
  '                let rebindable = !self.block_keeps_lexical_self(&recv_ty, &name);' \
  '                let rebindable = false;' \
  self_send_inside_a_rebindable_block_is_silent \
  'no block is ever rebindable: the silent fixture is shown to depend on that determination'

echo '--- restore and prove the shipped source is byte-identical'
restore
cmp -s "$CHK" "$BAK_CHK" && ok 'check.rs restored byte-identical' || bad 'source NOT restored'
run_suite
if (( ! compiles )); then bad 'rebuild of the shipped source failed'
elif [[ -n $failing ]]; then bad "shipped source is not green: $(tr '\n' ' ' <<<"$failing")"
else ok 'shipped source green after every mutant'; fi
trap - EXIT
rm -f "$BAK_CHK"

if (( fail )); then echo 'RESULT: FAIL (rebindable guard mutants)'; exit 1; fi
echo 'RESULT: PASS (rebindable guard mutants - 4 mutants, each accused by a named control)'