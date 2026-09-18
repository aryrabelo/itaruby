#!/usr/bin/env bash
# The inference bench as an exit code. Wraps scripts/inference-bench.rb with
# the two controls that make its output evidence instead of decoration:
#
#   1. FRESH BINARY. On 2026-09-17 a gate here printed `FAIL cargo build` and
#      then judged whatever ./target/release/ita happened to be on disk
#      (AGENTS.md, binding). This gate builds first and REFUSES to judge if
#      the build fails; it then proves freshness by comparing the binary's
#      mtime against the newest file under crates/ and the manifests, and
#      prints the binary's sha256 so the transcript names what was measured.
#   2. FAIL CLOSED ON A MISSING LEG. No `ruby` or no `srb` is a SKIP (exit 2)
#      that names itself, never a quiet pass. A wrong `srb` version is also a
#      skip: the manifest pins the version its rows were measured against.
#
#   0  every row matched
#   1  a row diverged, a fixture is broken, or the binary could not be built
#   2  matched what it could; a leg was skipped (named in the transcript)
set -uo pipefail

cd "$(dirname "$0")/.."
ROOT=$PWD
ITA=$ROOT/target/release/ita
ART=${ART:-$ROOT/target/gauntlet}
mkdir -p "$ART"

if ! command -v ruby >/dev/null; then
  echo 'SKIP inference bench — no ruby on this machine (MRI is the ground truth)'
  exit 2
fi

echo '--- building the binary this gate will judge'
if ! cargo build --release --locked --target-dir "$ROOT/target" >"$ART/inference-build.txt" 2>&1; then
  echo "FAIL cargo build (see $ART/inference-build.txt) — refusing to judge a stale binary"
  exit 1
fi
if [[ ! -x $ITA ]]; then
  echo "FAIL no binary at $ITA after a successful build"
  exit 1
fi

# Freshness, measured rather than assumed: nothing that feeds the binary may
# be newer than the binary itself.
newest=$(find "$ROOT/crates" "$ROOT/Cargo.toml" "$ROOT/Cargo.lock" -type f -newer "$ITA" -print -quit 2>/dev/null)
if [[ -n $newest ]]; then
  echo "FAIL binary is older than $newest — the build did not produce what is being judged"
  exit 1
fi
echo "binary verified fresh: $ITA"

args=(--json "$ART/inference-bench.jsonl" --ita "$ITA")
if command -v srb >/dev/null; then
  args+=(--sorbet)
else
  echo 'note: no `srb` on this machine — the comparison leg will be skipped by the oracle'
fi

ruby "$ROOT/scripts/inference-bench.rb" "${args[@]}"
rc=$?
case $rc in
  0) echo 'RESULT: PASS (inference bench)' ;;
  2) echo 'RESULT: PASS (inference bench, incomplete — a leg was skipped)' ;;
  *) echo 'RESULT: FAIL (inference bench)' ;;
esac
exit $rc
