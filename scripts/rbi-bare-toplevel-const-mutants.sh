#!/usr/bin/env bash
# Two-sided proof for bead ita-h5s: a bare, unqualified top-level
# value-constant write in a client's `sorbet/rbi/**` (planningalerts' real
# `ElasticSearchClient = T.let(T.unsafe(nil),
# T.nilable(Elasticsearch::Client))` shim shape) was never read by any RBI
# channel before this bead. The fix spans three files — `rbi.rs`'s phase-1
# scanner, `index.rs`'s phase-2 verifier, and `check.rs`'s wiring into the
# fallback chain — so this harness mutates all three in turn.
#
# Each mutant removes exactly ONE load-bearing decision and must be
# ACCUSED by a NAMED test in
# crates/itaruby_semantic/tests/rbi_bare_toplevel_const.rs. A mutant that
# compiles with the suite green is a missing control, not a safe change
# (AGENTS.md): add the fixture, then count the mutant.
#
#   MUT-A  `check.rs::rbi_fallback_declares` never tries
#          `rbi_toplevel_const_declares` at all
#          -> bare_toplevel_const_resolves_silently must fail: `XClient`
#             re-emits the exact false E0104 this bead exists to kill
#   MUT-B  `rbi.rs::build_rbi_index` never unions
#          `scan_toplevel_const_write`'s hits into the candidate map (only
#          `scan_class_name` runs) — same observable effect as MUT-A from a
#          different layer, proving the phase-1 step is load-bearing too
#          -> bare_toplevel_const_resolves_silently must fail
#   MUT-C  `index.rs::rbi_toplevel_const_declares`'s phase-2 check weakens
#          to "the candidate file exists" (drops the real
#          `toplevel_consts`/`Object`/`Kernel`-fragment membership test)
#          -> heredoc_body_false_positive_still_accuses must fail:
#             `FakeConst`, which only ever appears as a heredoc BODY line
#             (never a real `ConstantWriteNode`), wrongly resolves
#
# Builds/tests go to $ROOT/target: measuring what another target-dir
# produced is the 2026-09-17 stale-binary defect (AGENTS.md), and on a
# machine with a global `build.target-dir` two worktrees otherwise resolve
# each other's rmeta (2026-09-17).
set -uo pipefail

cd "$(dirname "$0")/.."
ROOT=$PWD
CHK=crates/itaruby_semantic/src/check.rs
RBI=crates/itaruby_semantic/src/rbi.rs
IDX=crates/itaruby_semantic/src/index.rs
BAK_CHK=$ROOT/target/rbi-bare-toplevel-const-check.rs.orig
BAK_RBI=$ROOT/target/rbi-bare-toplevel-const-rbi.rs.orig
BAK_IDX=$ROOT/target/rbi-bare-toplevel-const-index.rs.orig
LOG=$ROOT/target/rbi-bare-toplevel-const-mutants-test.txt

fail=0
ok()  { printf 'PASS %s\n' "$1"; }
bad() { printf 'FAIL %s\n' "$1"; fail=1; }

mkdir -p "$ROOT/target"
cp "$CHK" "$BAK_CHK"
cp "$RBI" "$BAK_RBI"
cp "$IDX" "$BAK_IDX"
restore() { cp "$BAK_CHK" "$CHK"; cp "$BAK_RBI" "$RBI"; cp "$BAK_IDX" "$IDX"; }
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
    --test rbi_bare_toplevel_const >"$LOG" 2>&1
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
  cmp -s "$CHK" "$BAK_CHK" || bad "$label check.rs NOT restored"
  cmp -s "$RBI" "$BAK_RBI" || bad "$label rbi.rs NOT restored"
  cmp -s "$IDX" "$BAK_IDX" || bad "$label index.rs NOT restored"
}

echo '--- baseline (shipped source)'
run_suite
if (( ! compiles )); then echo 'ABORT: shipped source does not compile'; exit 1; fi
if [[ -n $failing ]]; then
  echo "ABORT: baseline is not green — failing: $(tr '\n' ' ' <<<"$failing")"
  exit 1
fi
ok 'baseline green (bare top-level RBI const resolves silently, every control accuses)'

mutant MUT-A "$CHK" \
  '        crate::index::rbi_declares(path, map)
            || crate::index::rbi_qualified_const_declares(path, map)
            || crate::index::rbi_toplevel_const_declares(path, map)' \
  '        crate::index::rbi_declares(path, map)
            || crate::index::rbi_qualified_const_declares(path, map)' \
  bare_toplevel_const_resolves_silently \
  'the fallback chain never tries the bare top-level channel at all'

mutant MUT-B "$RBI" \
  '            let name = scan_class_name(&line).or_else(|| scan_toplevel_const_write(&line));' \
  '            let name = scan_class_name(&line);' \
  bare_toplevel_const_resolves_silently \
  'phase 1 never produces a candidate for a bare value-constant write — the file locator never even widens'

mutant MUT-C "$IDX" \
  '    paths.iter().any(|path| {
        let defs = rbi_file_defs(path);
        defs.toplevel_consts.iter().any(|c| c == name)
            || defs
                .fragments
                .iter()
                .any(|f| (f.path == "Object" || f.path == "Kernel") && f.consts.iter().any(|c| c == name))
    })' \
  '    let _ = name;
    !paths.is_empty()' \
  heredoc_body_false_positive_still_accuses \
  'phase 2 degrades to "the candidate file exists": a heredoc BODY line that merely looks like an assignment wrongly resolves'

echo '--- restore and prove the shipped source is byte-identical'
restore
cmp -s "$CHK" "$BAK_CHK" && ok 'check.rs restored byte-identical' || bad 'check.rs NOT restored'
cmp -s "$RBI" "$BAK_RBI" && ok 'rbi.rs restored byte-identical' || bad 'rbi.rs NOT restored'
cmp -s "$IDX" "$BAK_IDX" && ok 'index.rs restored byte-identical' || bad 'index.rs NOT restored'
run_suite
if (( ! compiles )); then bad 'rebuild of the shipped source failed'
elif [[ -n $failing ]]; then bad "post-restore: suite not green — failing: $(tr '\n' ' ' <<<"$failing")"
else ok 'post-restore: suite green again'
fi
trap - EXIT
rm -f "$BAK_CHK" "$BAK_RBI" "$BAK_IDX"

if (( fail )); then echo 'RESULT: FAIL (rbi-bare-toplevel-const mutants)'; exit 1; fi
echo 'RESULT: PASS (rbi-bare-toplevel-const mutants)'
