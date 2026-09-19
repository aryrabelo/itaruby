#!/usr/bin/env bash
# Two-sided proof for bead E (ita-w2c, phase A/onda 2): the asserted-raise
# softening, in the NARROW form the owner chose — only the diagnostic
# whose CALL is the direct subject of the assertion is silenced.
#
# Each mutant removes exactly ONE load-bearing decision and must be
# ACCUSED by a NAMED test in crates/itaruby_semantic/tests/asserted_raise.rs.
# A mutant that compiles with the suite green is a missing control, not a
# safe change (AGENTS.md): add the fixture, then count the mutant.
#
# M1 the RSpec arming is never installed around the receiver walk
#    -> rspec_expect_raise_error_subject_is_silent must fail
# M2 the minitest arming is never installed around the block walk
#    -> minitest_assert_raises_subject_is_silent must fail
# M3 the older singular `assert_raise` spelling stops counting
#    -> minitest_assert_raise_singular_spelling_is_silent must fail
# M4 any `.to <matcher>` arms, not just `raise_error`
#    -> expect_with_a_non_raise_matcher_still_accuses must fail
# M5 the suppression channel stops being consulted at all
#    -> rspec_expect_raise_error_subject_is_silent must fail
# M6 the armed span only has to CONTAIN the call, instead of BEING it —
#    one call wide becomes everything the subject's span contains
#    -> call_nested_inside_the_armed_subject_span_still_accuses must fail
#
# Unmutated, with the reason measured rather than assumed: the
# `direct_statement_call_spans` fallback arm (`body` that is itself a
# `CallNode`) is unreachable from Prism — a block body is always a
# `StatementsNode` (the silent fixtures suppress through the statements
# arm alone) — so no fixture can pin it and no mutant may pretend to.
#
# Builds/tests go to $ROOT/target: measuring what another target-dir
# produced is the 2026-09-17 stale-binary defect (AGENTS.md), and on a
# machine with a global `build.target-dir` two worktrees otherwise resolve
# each other's rmeta (2026-09-17).
set -uo pipefail

cd "$(dirname "$0")/.."
ROOT=$PWD
CHK=crates/itaruby_semantic/src/check.rs
BAK_CHK=$ROOT/target/asserted-raise-check.rs.orig
LOG=$ROOT/target/asserted-raise-mutants-test.txt

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
    --test asserted_raise >"$LOG" 2>&1
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
  '        let rspec_subjects = rspec_raise_subject_spans(call);
        let saved_subject_len = self.asserted_subject_spans.len();
        self.asserted_subject_spans.extend(rspec_subjects);' \
  '        let saved_subject_len = self.asserted_subject_spans.len();' \
  rspec_expect_raise_error_subject_is_silent \
  'the `expect { ... }.to raise_error` subject is never armed'

mutant M2 "$CHK" \
  '                let minitest_subjects = (name == "assert_raises" || name == "assert_raise")
                    .then(|| b.body().map(|body| direct_statement_call_spans(&body)))
                    .flatten()
                    .unwrap_or_default();' \
  '                let minitest_subjects: Vec<(usize, usize)> = Vec::new();' \
  minitest_assert_raises_subject_is_silent \
  'the `assert_raises { ... }` block subject is never armed'

mutant M3 "$CHK" \
  '(name == "assert_raises" || name == "assert_raise")' \
  '(name == "assert_raises")' \
  minitest_assert_raise_singular_spelling_is_silent \
  'minitest'"'"'s older singular spelling stops counting as the assertion'

mutant M4 "$CHK" \
  '    if matcher.name().as_slice() != b"raise_error" {
        return Vec::new();
    }' \
  '    // mutant: any matcher arms' \
  expect_with_a_non_raise_matcher_still_accuses \
  '`.to <any matcher>` arms: a non-raise assertion silences its subject'

mutant M5 "$CHK" \
  '        if self
            .asserted_subject_spans
            .iter()
            .any(|s| *s == span_of_call(call))
        {' \
  '        if self
            .asserted_subject_spans
            .iter()
            .any(|_| false)
        {' \
  rspec_expect_raise_error_subject_is_silent \
  'the armed span stops matching the call: the subject is diagnosed again'

mutant M6 "$CHK" \
  '            .any(|s| *s == span_of_call(call))' \
  '            .any(|s| s.0 <= span_of_call(call).0 && span_of_call(call).1 <= s.1)' \
  call_nested_inside_the_armed_subject_span_still_accuses \
  'the armed span only has to contain the call: a call nested inside the subject is silenced too'

echo '--- restore and prove the shipped source is byte-identical'
restore
cmp -s "$CHK" "$BAK_CHK" && ok 'check.rs restored byte-identical' || bad 'source NOT restored'
run_suite
if (( ! compiles )); then bad 'rebuild of the shipped source failed'
elif [[ -n $failing ]]; then bad "shipped source is not green: $(tr '\n' ' ' <<<"$failing")"
else ok 'shipped source green after every mutant'; fi
trap - EXIT
rm -f "$BAK_CHK"

if (( fail )); then echo 'RESULT: FAIL (asserted raise mutants)'; exit 1; fi
echo 'RESULT: PASS (asserted raise mutants - 6 mutants, each accused by a named control)'