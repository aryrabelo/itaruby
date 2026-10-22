#!/usr/bin/env bash
# Two-sided proof for scripts/sorbet-capability-probes.rb (AGENTS.md: the
# instrument that produces evidence gets the same treatment as the code it
# judges).
#
#   clean      the shipped script against the real tools exits 0, every
#              probe PASS — it stays silent on a matrix that is true.
#   MUT-EXPECT one expectation flipped (refinement: Sorbet "silent") must
#              exit 1 with exactly one FAIL line, naming `refinement`.
#   MUT-PARSE  the srb code parser broken must exit 1, accusing
#              `undef_instance` — a blind parser cannot read as "all silent".
#   MUT-VERSION a different pinned version must SKIP (exit 2), never judge.
#   MUT-TOOL   an `ita` that reports nothing must exit 1, accusing
#              `undef_instance` and `lsp_capabilities`.
#   MUT-NOSRB  a missing srb must SKIP (exit 2).
#
# Mutants run on a copy written NEXT TO the script (ROOT is derived from
# the script's own directory, so a copy elsewhere would not find the repo)
# and the shipped file is proved byte-identical with cmp afterwards.
# Requires srb at the pinned version; without it, exit 2.
set -uo pipefail

cd "$(dirname "$0")/.."
ROOT=$PWD
SCRIPT=scripts/sorbet-capability-probes.rb
MUTANT=scripts/.sorbet-capability-probes.mutant.rb
WORK=$ROOT/target/sorbet-capability-probes-selftest
ITA=$ROOT/target/release/ita
mkdir -p "$WORK"
cp "$SCRIPT" "$WORK/pristine.rb"
trap 'rm -f "$MUTANT"' EXIT

fail=0
accuse() { echo "FAIL $1"; fail=1; }

expect() { # name want_rc log [must_contain...]
  local name=$1 want=$2 log=$3 rc=$4
  shift 4
  if [[ $rc != "$want" ]]; then accuse "$name: exit $rc, wanted $want (log $log)"; return; fi
  local needle
  for needle in "$@"; do
    grep -qE -- "$needle" "$log" || { accuse "$name: transcript lacks /$needle/ (log $log)"; return; }
  done
  echo "PASS $name"
}

mutate() { # anchor replacement
  local count
  count=$(ANCHOR=$1 ruby -e 'print File.read(ARGV[0]).scan(ENV["ANCHOR"]).size' "$SCRIPT")
  if [[ $count != 1 ]]; then
    echo "INVALIDO anchor matched $count times: $1"
    exit 1
  fi
  ANCHOR=$1 REPL=$2 ruby -e 'print File.read(ARGV[0]).sub(ENV["ANCHOR"]) { ENV["REPL"] }' "$SCRIPT" >"$MUTANT"
}

# clean (also builds the binary every later case reuses)
ruby "$SCRIPT" >"$WORK/clean.log" 2>&1
rc=$?
if [[ $rc == 2 ]]; then
  cat "$WORK/clean.log"
  echo 'SKIP selftest — no srb at the pinned version'
  exit 2
fi
expect clean 0 "$WORK/clean.log" "$rc" '^RESULT: PASS'
grep -q '^FAIL' "$WORK/clean.log" && accuse 'clean: a probe failed on the shipped script'

mutate "['refinement', 'true', <<~RUBY, %w[7003], []]" "['refinement', 'true', <<~RUBY, [], []]"
ruby "$MUTANT" --ita "$ITA" >"$WORK/mut-expect.log" 2>&1
expect MUT-EXPECT 1 "$WORK/mut-expect.log" "$?" '^FAIL refinement '
fails=$(grep -c '^FAIL ' "$WORK/mut-expect.log")
[[ $fails == 1 ]] || accuse "MUT-EXPECT: $fails FAIL lines, wanted exactly 1"

mutate 'https://srb\.help/(\d+)' 'https://srb\.helpX/(\d+)'
ruby "$MUTANT" --ita "$ITA" >"$WORK/mut-parse.log" 2>&1
expect MUT-PARSE 1 "$WORK/mut-parse.log" "$?" '^FAIL undef_instance '

mutate "SRB_VERSION = '0.6.13437'" "SRB_VERSION = '0.0.1'"
ruby "$MUTANT" --ita "$ITA" >"$WORK/mut-version.log" 2>&1
expect MUT-VERSION 2 "$WORK/mut-version.log" "$?" '^SKIP srb is not 0\.0\.1'

printf '#!/bin/sh\nexit 0\n' >"$WORK/silent-ita"
chmod +x "$WORK/silent-ita"
ruby "$SCRIPT" --ita "$WORK/silent-ita" >"$WORK/mut-tool.log" 2>&1
expect MUT-TOOL 1 "$WORK/mut-tool.log" "$?" '^FAIL undef_instance ' '^FAIL lsp_capabilities '

ruby "$SCRIPT" --ita "$ITA" --srb "$WORK/no-such-srb" >"$WORK/mut-nosrb.log" 2>&1
expect MUT-NOSRB 2 "$WORK/mut-nosrb.log" "$?" '^SKIP no '

cmp -s "$SCRIPT" "$WORK/pristine.rb" || accuse 'the shipped script changed during the selftest'

if [[ $fail == 0 ]]; then
  echo 'RESULT: PASS (sorbet-capability-probes selftest)'
else
  echo 'RESULT: FAIL (sorbet-capability-probes selftest)'
fi
exit $fail
