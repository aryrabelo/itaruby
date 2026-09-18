#!/usr/bin/env bash
# Performance baseline gate. Runs the criterion benches and fails when a
# measured median exceeds the ceiling recorded in scripts/perf-baseline.txt.
#
# Why a ceiling file and not criterion's own baseline comparison: criterion
# stores baselines under target/, which is gitignored, so a fresh checkout
# or a CI runner has nothing to compare against — a benchmark with no
# reference measures nothing. A committed ceiling is the same mechanism
# scripts/corpus-baseline.txt already uses for diagnostics, and it survives
# a clean clone.
#
# Two columns because a hosted runner is not a measurement instrument: it
# is shared, throttled, and several times slower than a dev machine. The
# loose CI column exists to catch an order-of-magnitude regression, which
# is the failure this gate is really for; the tight dev column is the one
# that ratchets.
#
# Slack is debt, not margin (AGENTS.md, binding): when the dev column
# measures comfortably below its ceiling, tighten the ceiling in the same
# commit.
set -euo pipefail

cd "$(dirname "$0")/.."

BASELINE=scripts/perf-baseline.txt

# The column is asked for, never inferred. Keying off `$CI` looked obvious
# and was a trap: plenty of local shells and agent harnesses export `CI`,
# and the failure is silent in the wrong direction — the gate quietly
# compares a dev machine against the loose ceiling and passes a regression
# it exists to catch. Measured on this repository the first time this ran.
COLUMN=${PERF_COLUMN:-dev}
case $COLUMN in
  ci | dev) ;;
  *)
    echo "perf gate: FAIL — PERF_COLUMN must be 'ci' or 'dev', got '$COLUMN'" >&2
    exit 1
    ;;
esac

# `--no-run`: compare only, against measurements a previous step already
# produced. CI uses it so that the workflow itself names `cargo bench` on
# the line that runs it, rather than hiding the tool one level down inside
# this script — a reader of the workflow, and the rule that scans it, both
# see the same thing.
RUN_BENCH=1
if [ "${1:-}" = "--no-run" ]; then RUN_BENCH=0; fi

echo "perf gate: column=$COLUMN run_bench=$RUN_BENCH"

if [ ! -f "$BASELINE" ]; then
  echo "perf gate: FAIL — $BASELINE missing; a gate with no ceiling proves nothing" >&2
  exit 1
fi

# `--target-dir`, explicitly: the python comparison below reads
# `target/criterion/<id>/new/estimates.json` relative to the repo, and a
# machine with a global `build.target-dir` in ~/.cargo/config.toml writes
# criterion's output somewhere else entirely — measured here, the gate
# then reported "produced no estimate" for every row while the bench had
# run fine. `--target-dir` alone is NOT enough: criterion resolves its
# own output root from `CRITERION_HOME`/`CARGO_TARGET_DIR`, so the flag
# moved the build artifacts and left the measurements in the global dir.
# Same defect family as the 2026-09-17 build/capture mismatch (AGENTS.md,
# binding): an evidence producer must write where the reader looks.
if [ "$RUN_BENCH" = "1" ]; then
  CRITERION_HOME="$PWD/target/criterion" \
    cargo bench -p itaruby_semantic --bench check --target-dir "$PWD/target" -- --noplot \
    >/dev/null 2>&1 || {
    echo "perf gate: FAIL — the benchmark itself did not run" >&2
    exit 1
  }
fi

python3 - "$BASELINE" "$COLUMN" <<'PY'
import json, os, sys

baseline_path, column = sys.argv[1], sys.argv[2]
col = 1 if column == "ci" else 2

rows = []
for line in open(baseline_path, encoding="utf-8"):
    line = line.split("#", 1)[0].strip()
    if line:
        parts = line.split()
        if len(parts) != 3:
            sys.exit(f"perf gate: FAIL - malformed baseline row: {parts}")
        rows.append((parts[0], int(parts[col])))

if not rows:
    sys.exit("perf gate: FAIL - baseline file declares no benchmark")

failed = False
for bench_id, ceiling_ns in rows:
    est = os.path.join("target", "criterion", bench_id, "new", "estimates.json")
    if not os.path.exists(est):
        print(f"perf gate: FAIL - {bench_id} produced no estimate at {est}")
        failed = True
        continue
    with open(est, encoding="utf-8") as fh:
        median_ns = json.load(fh)["median"]["point_estimate"]
    pct = 100.0 * median_ns / ceiling_ns
    verdict = "ok" if median_ns <= ceiling_ns else "OVER CEILING"
    print(
        f"  {bench_id}: {median_ns / 1e6:.2f} ms "
        f"(ceiling {ceiling_ns / 1e6:.2f} ms, {pct:.0f}% of it) {verdict}"
    )
    if median_ns > ceiling_ns:
        failed = True

if failed:
    sys.exit("perf gate: FAIL")
print("perf gate: PASS")
PY
