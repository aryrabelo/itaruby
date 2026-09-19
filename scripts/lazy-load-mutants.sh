#!/usr/bin/env bash
# Two-sided proof for bead B (onda 2): `run_load_hooks(<literal symbol>,
# <literal base>)` marks the NAMED base's instance surface open. AGENTS.md,
# binding: the probes proving each side live next to the instrument, never
# only narrated — this file is where the mutation matrix lives instead of
# in a commit message.
#
# Each mutant removes exactly ONE load-bearing decision and must be
# ACCUSED by a NAMED test in crates/itaruby_semantic/tests/lazy_load.rs.
# A mutant that compiles with the suite green is a missing control, not a
# safe change (AGENTS.md): add the fixture, then count the mutant.
#
#   MUT-A  the whole registration stops firing (the call name is never
#          `run_load_hooks`)
#          -> run_load_hooks_base_resolves_silently must fail: nothing is
#             opened and the fixture's base goes back to conclusive
#             NotFound (this is the rails FakeContext line)
#   MUT-B  the two-argument gate loosens to "at least two"
#          -> three_argument_call_opens_nothing must fail: a call that is
#             not the library's shape opens the class it mentions second
#   MUT-C  the literal-symbol gate on the hook NAME drops
#          -> non_literal_hook_name_opens_nothing must fail: an
#             unfollowable registry opens the class anyway
#   MUT-D  the call site's lexical nesting is dropped
#          -> run_load_hooks_base_resolves_silently must fail: `Base` is
#             written RELATIVE to its enclosing class (the rails shape),
#             so an empty nesting resolves nothing
#   MUT-E  the resolve-and-open pass stops being wired into project_index
#          -> run_load_hooks_base_resolves_silently must fail: a
#             mechanism nothing calls is not a safeguard
#   MUT-F  the open flag itself is not set (only the census reason is)
#          -> run_load_hooks_base_resolves_silently must fail:
#             `lookup_method` reads `class.open`, not the reason
#
# Builds/tests go to $ROOT/target: measuring what another target-dir
# produced is the 2026-09-17 stale-binary defect (AGENTS.md), and on a
# machine with a global `build.target-dir` two worktrees otherwise
# resolve each other's rmeta (2026-09-17).
set -uo pipefail

cd "$(dirname "$0")/.."
ROOT=$PWD
IDX=crates/itaruby_semantic/src/index.rs
BAK_IDX=$ROOT/target/lazy-load-index.rs.orig
LOG=$ROOT/target/lazy-load-mutants-test.txt

fail=0
ok()  { printf 'PASS %s\n' "$1"; }
bad() { printf 'FAIL %s\n' "$1"; fail=1; }

mkdir -p "$ROOT/target"
cp "$IDX" "$BAK_IDX"
restore() { cp "$BAK_IDX" "$IDX"; }
trap restore EXIT

# One test-binary run over the suite these mutants can be accused by.
compiles=0
failing=
# Sets `compiles` and `failing` in the CALLER's shell — never called in a
# command substitution, because a subshell's assignments die with it and
# "did not compile" would read as "green".
run_suite() {
  # `--no-fail-fast` is load-bearing, not hygiene: cargo stops after the
  # first failing test BINARY, so without it a mutant whose control lives
  # in a later suite reported "expected X to fail" while X had never run.
  CARGO_TARGET_DIR="$ROOT/target" cargo test -p itaruby_semantic --no-fail-fast \
    --test lazy_load >"$LOG" 2>&1
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
  cmp -s "$IDX" "$BAK_IDX" || bad "$label source NOT restored byte-identical"
}

echo '--- baseline (shipped source)'
run_suite
if (( ! compiles )); then echo 'ABORT: shipped source does not compile'; exit 1; fi
if [[ -n $failing ]]; then
  echo "ABORT: baseline is not green — failing: $(tr '\n' ' ' <<<"$failing")"
  exit 1
fi
ok 'baseline green (the silent fixture is silent, every control accuses)'

mutant MUT-A "$IDX" \
  '        if node.name().as_slice() != b"run_load_hooks" {
            return;
        }' \
  '        if true {
            return;
        }' \
  run_load_hooks_base_resolves_silently \
  'the registration never fires: the base keeps a conclusive NotFound surface'

mutant MUT-B "$IDX" \
  '        if args.len() != 2 || args.iter().next().and_then(|a| a.as_symbol_node()).is_none() {
            return;
        }' \
  '        if args.iter().next().and_then(|a| a.as_symbol_node()).is_none() {
            return;
        }' \
  three_argument_call_opens_nothing \
  'the arity gate loosens: a three-argument call opens the class it mentions second'

mutant MUT-C "$IDX" \
  '        if args.len() != 2 || args.iter().next().and_then(|a| a.as_symbol_node()).is_none() {
            return;
        }' \
  '        if args.len() != 2 {
            return;
        }' \
  non_literal_hook_name_opens_nothing \
  'the literal-symbol gate drops: a hook name held in a variable opens the base anyway'

mutant MUT-D "$IDX" \
  '        self.load_hook_bases.push((base, self.nesting.clone()));' \
  '        self.load_hook_bases.push((base, Vec::new()));' \
  run_load_hooks_base_resolves_silently \
  'the call site nesting is dropped: a base named relative to its enclosing class resolves to nothing'

mutant MUT-E "$IDX" \
  '    apply_load_hook_openness(&mut index);
' \
  '' \
  run_load_hooks_base_resolves_silently \
  'the pass stops being wired into project_index: nothing ever opens'

mutant MUT-F "$IDX" \
  '    let class = &mut index.classes[id.0 as usize];
    class.open = true;' \
  '    let class = &mut index.classes[id.0 as usize];' \
  run_load_hooks_base_resolves_silently \
  'the open flag is not set: `lookup_method` reads `class.open`, not the recorded reason'

echo '--- restore and prove the shipped source is byte-identical'
restore
cmp -s "$IDX" "$BAK_IDX" && ok 'index.rs restored byte-identical' || bad 'source NOT restored'
run_suite
if (( ! compiles )); then bad 'rebuild of the shipped source failed'
elif [[ -n $failing ]]; then
  bad "post-restore suite not green: $(tr '\n' ' ' <<<"$failing")"
else
  ok 'post-restore suite green again'
fi
trap - EXIT
rm -f "$BAK_IDX"

if (( fail )); then echo 'RESULT: FAIL (lazy-load mutants)'; exit 1; fi
echo 'RESULT: PASS (lazy-load mutants)'