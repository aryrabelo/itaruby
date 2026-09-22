#!/usr/bin/env bash
# instrument-mutants.sh — the two-sided proof that this repo's EVIDENCE
# instruments are not blind. Not about the checker: about the scripts that
# produce the evidence the checker is judged by.
#
# Three defects, each measured on 2026-09-17 and each fixed in the same
# commit as this harness:
#
#   gauntlet-fail-fast   gauntlet-gates.sh ran binary-consuming gates with a
#                        STALE ./target/release/ita after `cargo build` failed
#   replay-run-isolation replay.sh appended every run into one results/<id>.jsonl,
#                        so a MISS from a previous day read as today's result
#   replay-build-pin     replay.sh built into cargo's GLOBAL target dir while
#                        executing $ROOT/target/release/ita — measuring a binary
#                        the checked build never produced
#
# Each case asserts BOTH sides: the mutant reproduces the historical defect,
# and the shipped script does not. `cmp` guards every mutation (a sed that
# matches nothing prints "zero accusations", which is indistinguishable from
# an instrument that does not fire).
#
#   0  every case proved both sides
#   1  a case failed (named in the transcript)
set -uo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD
#
# The lab is RECREATED on every invocation. The default path carries a
# timestamp and so was always unique, but gauntlet-gates.sh passes a FIXED
# path ($ART/instrument-mutants), so every gate run appended into the
# previous run's lab and `replay-run-isolation` measured 4, then 6, then N
# records where it expects 2 — the harness failing the exact "two runs
# shared one file" defect that case exists to detect (measured
# 2026-09-17). A lab that survives its own run is not a lab.
#
# A RELATIVE path is refused outright rather than resolved silently: this
# script derives everything from $ROOT and then `cd`s inside the cases, so
# a relative lab resolves differently depending on who is looking, and the
# observed result was three cases reporting "did not reproduce" with an
# empty witness — a broken fixture wearing the words of a real finding
# (AGENTS.md, binding).
LAB=${INSTRUMENT_MUTANTS_LAB:-$HOME/Sites/temp-files/instrument-mutants-$(date +%Y%m%d-%H%M%S)}
failed=0
say()  { printf '\n=== %s\n' "$1"; }
ok()   { printf 'PASS %s\n' "$1"; }
bad()  { printf 'FAIL %s\n' "$1"; failed=1; }

if [[ $LAB != /* ]]; then
  printf 'FAIL INSTRUMENT_MUTANTS_LAB must be an ABSOLUTE path, got %q\n' "$LAB" >&2
  printf '     (a relative lab resolves differently inside each case and makes\n' >&2
  printf '      every case report "did not reproduce" with an empty witness)\n' >&2
  exit 1
fi
case $LAB in
  / | "$ROOT" | "$ROOT"/)
    printf 'FAIL INSTRUMENT_MUTANTS_LAB refuses to use %q as a scratch lab\n' "$LAB" >&2
    exit 1
    ;;
esac
rm -rf "$LAB"
mkdir -p "$LAB" || { printf 'FAIL could not create lab %q\n' "$LAB" >&2; exit 1; }
echo "lab: $LAB (recreated fresh for this run)"

# git subprocesses must never inherit a hook's GIT_DIR/GIT_INDEX_FILE: in a
# linked worktree those beat `cwd` and the probe would act on the real repo.
git_clean() { env -u GIT_DIR -u GIT_INDEX_FILE -u GIT_WORK_TREE -u GIT_PREFIX \
  -u GIT_COMMON_DIR -u GIT_AUTHOR_DATE -u GIT_COMMITTER_DATE git "$@"; }

# mutate FILE_IN FILE_OUT 'literal-needle' 'literal-replacement'
mutate() {
  python3 - "$@" <<'PY'
import sys, pathlib
src = pathlib.Path(sys.argv[1]).read_text()
out = src.replace(sys.argv[3], sys.argv[4])
if out == src:
    sys.exit(3)
pathlib.Path(sys.argv[2]).write_text(out)
PY
}

# ---------- case 1: gauntlet fail-fast ----------
say 'case gauntlet-fail-fast — a failed release build must not run any binary-consuming gate'
c1=$LAB/gauntlet
mk_gauntlet() { # DIR SCRIPT_SRC
  mkdir -p "$1/scripts" "$1/target/release" "$1/fakebin" "$1/art"
  cp "$2" "$1/scripts/gauntlet-gates.sh"
  printf '# no corpus declared in this lab\n' >"$1/scripts/corpus-baseline.txt"
  printf '#!/bin/sh\n[ "$1" = build ] && { echo "injected build failure" >&2; exit 7; }\nexit 0\n' >"$1/fakebin/cargo"
  printf '#!/bin/sh\nexit 0\n' >"$1/fakebin/sf"
  printf '#!/bin/sh\nprintf "STALE\\n" >>"$PROBE_WITNESS"\nexit 1\n' >"$1/target/release/ita"
  for s in unwrap-gate perf-gate public-gate; do printf '#!/bin/sh\nexit 0\n' >"$1/scripts/$s.sh"; done
  chmod +x "$1/fakebin/"* "$1/target/release/ita" "$1/scripts/"*.sh
}
run_gauntlet() { # DIR -> echoes witness content
  local w=$1/witness.txt
  rm -f "$w"
  ( cd "$1" && PATH="$1/fakebin:$PATH" PROBE_WITNESS="$w" ART="$1/art" \
      bash "$1/scripts/gauntlet-gates.sh" >"$1/log.txt" 2>&1 )
  [[ -f $w ]] && cat "$w" || true
}
mkdir -p "$c1"
mk_gauntlet "$c1/shipped" "$ROOT/scripts/gauntlet-gates.sh"
# The two needles below are anchored on the build-failure branch of
# `gauntlet-gates.sh` AS IT READS TODAY. That branch was refactored to the
# `out` helper and grew a `digest` call, which silently drifted the old
# `echo` needle and turned this case into INVALIDO-cmp — the guard doing its
# job, but on a red gate nobody had re-run. Re-count both after ANY refactor
# of that branch (rules of proof: a disambiguated anchor is a NEW anchor).
# `digest` never exits (`|| true`), so removing the `exit 1` alone is what
# lets the mutant walk on into the binary-consuming gates.
if ! mutate "$ROOT/scripts/gauntlet-gates.sh" "$LAB/gauntlet-mutant.sh" \
      "  out 'RESULT: FAIL (build failed; no binary-consuming gates executed)'" \
      "  : # mutant: fail-fast announcement removed"; then
  bad 'gauntlet-fail-fast: mutation did not apply (INVALIDO-cmp)'
else
  mutate "$LAB/gauntlet-mutant.sh" "$LAB/gauntlet-mutant.sh" \
    $'  : # mutant: fail-fast announcement removed\n  digest\n  exit 1' \
    '  : # mutant: fail-fast removed entirely' \
    || bad 'gauntlet-fail-fast: second mutation did not apply (INVALIDO-cmp)'
  bash -n "$LAB/gauntlet-mutant.sh" || bad 'gauntlet-fail-fast: mutant does not parse (INVALIDO-parse)'
  mk_gauntlet "$c1/mutant" "$LAB/gauntlet-mutant.sh"
  shipped=$(run_gauntlet "$c1/shipped")
  mutant=$(run_gauntlet "$c1/mutant")
  if [[ -n $mutant ]]; then ok 'mutant reproduces the defect (stale binary ran after a failed build)'
  else bad 'mutant did NOT reproduce the defect — this case proves nothing'; fi
  if [[ -z $shipped ]]; then ok 'shipped script never runs the stale binary'
  else bad "shipped script ran the stale binary: $shipped"; fi
fi

# ---------- shared fixture for the replay cases ----------
say 'fixture — a disposable corpus clone with a real buggy->fixed commit pair'
corpora=$LAB/corpora; clone=$corpora/rails
mkdir -p "$clone"
git_clean init -q "$clone"
git_clean -C "$clone" config user.email probe@lab
git_clean -C "$clone" config user.name probe
printf 'class W\n  def dump\n    total\n  end\nend\n' >"$clone/f.rb"
git_clean -C "$clone" add -A && git_clean -C "$clone" commit -qm buggy
PARENT_SHA=$(git_clean -C "$clone" rev-parse HEAD)
printf 'class W\n  def dump\n    1\n  end\nend\n' >"$clone/f.rb"
git_clean -C "$clone" add -A && git_clean -C "$clone" commit -qm fixed
FIX_SHA=$(git_clean -C "$clone" rev-parse HEAD)
if [[ -n $PARENT_SHA && -n $FIX_SHA && $PARENT_SHA != "$FIX_SHA" ]]; then
  ok "fixture pair ${PARENT_SHA:0:8} -> ${FIX_SHA:0:8}"
else
  bad 'fixture: could not build a two-commit pair'; echo 'RESULT: FAIL'; exit 1
fi

mk_replay() { # DIR SCRIPT_SRC
  mkdir -p "$1/scripts/bug-replay/candidates" "$1/target/release" "$1/fakebin"
  cp "$2" "$1/scripts/bug-replay/replay.sh"
  cp "$ROOT/scripts/bug-replay/judge.rb" "$1/scripts/bug-replay/judge.rb"
  python3 - "$1/scripts/bug-replay/candidates/rails.jsonl" "$FIX_SHA" "$PARENT_SHA" <<'PY'
import json, sys
path, fix, parent = sys.argv[1:4]
open(path, 'w').write(json.dumps({
  "id": "rails-1-lab", "fix_sha": fix, "parent_sha": parent, "subject": "lab pair",
  "files": ["f.rb"],
  "hunks": [{"path": "f.rb", "old_start": 3, "new_start": 3,
             "removed": ["    total"], "added": ["    1"]}],
  "signal": "message"}) + "\n")
PY
  git_clean init -q "$1"
  git_clean -C "$1" config user.email probe@lab
  git_clean -C "$1" config user.name probe
  git_clean -C "$1" add -A >/dev/null 2>&1
  git_clean -C "$1" commit -qm seed >/dev/null 2>&1
  # The lab fabricates the DISK PRECONDITION the replay cases need, the
  # same way it already fabricates `cargo` and `ita`: replay.sh refuses to
  # create a worktree under a 6 GiB floor, so on a machine below the floor
  # both replay cases would report "mutant did NOT reproduce" with an empty
  # witness — a broken fixture wearing the words of a real finding
  # (AGENTS.md, binding). This `df` always answers "plenty".
  #
  # It fabricates the RESOURCE, never the PORTABILITY: it is a strict POSIX
  # `df`, so it rejects the BSD-only `-g` that GNU `df` also rejects, and
  # refuses any path outside this lab the way a Linux box refuses
  # /System/Volumes/Data. Those were exactly the two macOS assumptions that
  # made replay.sh judge NOTHING on the first Linux run of the gauntlet
  # workflow (2026-09-22) while this harness blamed the mutant. Written
  # this way the stub ACCUSES that defect on every platform instead of
  # hiding it: reintroduce either assumption and both replay cases go red.
  cat >"$1/fakebin/df" <<'DF'
#!/bin/sh
# An unset root would make the path guard below a bare `*` — permissive,
# and silently so. Refuse instead.
: "${LAB_DF_ROOT:?lab df: LAB_DF_ROOT unset}"
for a in "$@"; do
  case $a in
    -P | -k | -Pk | -kP) ;;
    -*) echo "df: invalid option -- '${a#-}'" >&2; exit 1 ;;
    "$LAB_DF_ROOT"*) ;;
    *) echo "df: $a: No such file or directory" >&2; exit 1 ;;
  esac
done
echo 'Filesystem 1024-blocks Used Available Capacity Mounted on'
echo 'lab 104857600 1048576 103809024 1% /'
DF
  chmod +x "$1/fakebin/df"
}
run_replay() { # DIR TAG
  ( cd "$1" && PATH="$1/fakebin:$PATH" \
      BUG_REPLAY_CORPORA="$corpora" BUG_REPLAY_WORKTREES="$LAB/wts-$2" \
      PROBE_WITNESS="$1/witness.txt" LAB_DF_ROOT="$LAB" \
      bash "$1/scripts/bug-replay/replay.sh" --repos rails --limit 1 --skip-srb \
      >>"$1/log.txt" 2>&1 )
}

# ---------- case 2: replay run isolation ----------
say 'case replay-run-isolation — two invocations must never share one results file'
c2=$LAB/isolation
if ! mutate "$ROOT/scripts/bug-replay/replay.sh" "$LAB/replay-mutant-isolation.sh" \
      '>>"$RUN_DIR/$id.jsonl"' '>>"$RESULTS/$id.jsonl"'; then
  bad 'replay-run-isolation: mutation did not apply (INVALIDO-cmp)'
else
  bash -n "$LAB/replay-mutant-isolation.sh" || bad 'replay-run-isolation: mutant does not parse (INVALIDO-parse)'
  mkdir -p "$c2"
  mk_replay "$c2/shipped" "$ROOT/scripts/bug-replay/replay.sh"
  mk_replay "$c2/mutant"  "$LAB/replay-mutant-isolation.sh"
  cp "$ROOT/target/release/ita" "$c2/shipped/target/release/ita"
  cp "$ROOT/target/release/ita" "$c2/mutant/target/release/ita"
  printf '#!/bin/sh\nexit 0\n' >"$c2/shipped/fakebin/cargo"; chmod +x "$c2/shipped/fakebin/cargo"
  printf '#!/bin/sh\nexit 0\n' >"$c2/mutant/fakebin/cargo";  chmod +x "$c2/mutant/fakebin/cargo"
  run_replay "$c2/shipped" s1; sleep 1; run_replay "$c2/shipped" s2
  run_replay "$c2/mutant"  m1; sleep 1; run_replay "$c2/mutant"  m2
  count_lines() { [[ -f $1 ]] && wc -l <"$1" | tr -d ' ' || echo 0; }
  mut_global=$(count_lines "$c2/mutant/scripts/bug-replay/results/rails.jsonl")
  ship_global=$(count_lines "$c2/shipped/scripts/bug-replay/results/rails.jsonl")
  ship_runs=$(find "$c2/shipped/scripts/bug-replay/results" -maxdepth 1 -name '*-run' | wc -l | tr -d ' ')
  ship_per_run=$(for f in "$c2/shipped/scripts/bug-replay/results/"*-run/rails.jsonl; do count_lines "$f"; done | sort -u | tr '\n' ',')
  if [[ $mut_global -eq 2 ]]; then ok "mutant reproduces the defect (2 runs mixed into one file: $mut_global lines)"
  else bad "mutant did NOT reproduce the defect (global file has $mut_global lines, expected 2)"; fi
  if [[ $ship_global -eq 0 && $ship_runs -eq 2 && $ship_per_run == '1,' ]]; then
    ok "shipped script isolates runs ($ship_runs run dirs, 1 record each, global file absent)"
  else
    bad "shipped script did not isolate runs (global=$ship_global runs=$ship_runs per_run=$ship_per_run)"
  fi
fi

# ---------- case 3: replay build pin ----------
say 'case replay-build-pin — the measured binary must be the one the checked build produced'
c3=$LAB/buildpin
if ! mutate "$ROOT/scripts/bug-replay/replay.sh" "$LAB/replay-mutant-pin.sh" \
      'cargo build --release --locked --target-dir "$ROOT/target" >"$RUN_DIR/build.txt"' \
      'cargo build --release >"$RUN_DIR/build.txt"'; then
  bad 'replay-build-pin: mutation did not apply (INVALIDO-cmp)'
else
  bash -n "$LAB/replay-mutant-pin.sh" || bad 'replay-build-pin: mutant does not parse (INVALIDO-parse)'
  mkdir -p "$c3"
  # this cargo only delivers a binary when the build is PINNED; unpinned it
  # "succeeds" elsewhere, leaving the stale binary in $ROOT/target to be measured.
  cargo_pin_probe='#!/bin/sh
[ "$1" = build ] || exit 0
case "$*" in
  *--locked*--target-dir*) : ;;
  *) echo "unpinned build went elsewhere" >&2; exit 0 ;;
esac
prev=""; td=""
for a in "$@"; do [ "$prev" = "--target-dir" ] && td=$a; prev=$a; done
mkdir -p "$td/release"
printf "#!/bin/sh\nprintf \"FRESH\\\\n\" >>\"\$PROBE_WITNESS\"\nexit 0\n" >"$td/release/ita"
chmod +x "$td/release/ita"
'
  for side in shipped mutant; do
    src=$ROOT/scripts/bug-replay/replay.sh
    [[ $side == mutant ]] && src=$LAB/replay-mutant-pin.sh
    mk_replay "$c3/$side" "$src"
    printf '%s' "$cargo_pin_probe" >"$c3/$side/fakebin/cargo"; chmod +x "$c3/$side/fakebin/cargo"
    printf '#!/bin/sh\nprintf "STALE\\n" >>"$PROBE_WITNESS"\nexit 0\n' >"$c3/$side/target/release/ita"
    chmod +x "$c3/$side/target/release/ita"
    run_replay "$c3/$side" "pin-$side"
  done
  ship_w=$(sort -u "$c3/shipped/witness.txt" 2>/dev/null | tr '\n' ',')
  mut_w=$(sort -u "$c3/mutant/witness.txt" 2>/dev/null | tr '\n' ',')
  if [[ $mut_w == *STALE* ]]; then ok "mutant reproduces the defect (measured the stale binary: $mut_w)"
  else bad "mutant did NOT reproduce the defect (witness: ${mut_w:-empty})"; fi
  if [[ $ship_w == 'FRESH,' ]]; then ok 'shipped script measures only the freshly built binary'
  else bad "shipped script measured something else (witness: ${ship_w:-empty})"; fi
fi

# ---------- case 4: public-gate artifact dir ----------
# Measured 2026-09-17: public-gate.sh never created $ART, so on a tree
# with no target/gauntlet every `>"$ART/..."` redirection failed, `comm`
# compared two files that do not exist, and the gate printed "public
# errors match baseline exactly" for every repo in ~0.01s and exited
# clean. The lab below is a miniature of the real gate: one one-commit
# git clone declared as a corpus, a baseline holding exactly the line the
# fake `ita` emits, and a ceiling nothing can exceed.
say 'case public-gate-artifacts — a comparison against artifacts that were never written must FAIL'
c4=$LAB/public-gate
pg_clone=$c4/corpora/lab
mkdir -p "$pg_clone"
git_clean init -q "$pg_clone"
git_clean -C "$pg_clone" config user.email probe@lab
git_clean -C "$pg_clone" config user.name probe
printf 'class L\n  def a\n    1\n  end\nend\n' >"$pg_clone/f.rb"
git_clean -C "$pg_clone" add -A && git_clean -C "$pg_clone" commit -qm lab
pg_sha=$(git_clean -C "$pg_clone" rev-parse HEAD)
mk_public() { # DIR SCRIPT_SRC
  mkdir -p "$1/scripts/baseline" "$1/target/release"
  cp "$2" "$1/scripts/public-gate.sh"
  chmod +x "$1/scripts/public-gate.sh"
  printf 'lab file:///dev/null %s 999\n' "$pg_sha" >"$1/scripts/corpora.txt"
  printf '{"code":"E0101","column":5,"line":3,"message":"lab","path":"lab/f.rb","severity":"error"}\n' \
    >"$1/scripts/baseline/lab.jsonl"
  # A fake `ita` that emits exactly one JSON diagnostic line, pathed
  # inside the lab clone so the gate's own normalization rewrites it to
  # `lab/f.rb` and the baseline above matches.
  {
    echo '#!/bin/sh'
    echo "cat <<'JSON'"
    printf '{"code":"E0101","column":5,"line":3,"message":"lab","path":"%s/f.rb","severity":"error"}\n' "$pg_clone"
    echo 'JSON'
  } >"$1/target/release/ita"
  chmod +x "$1/target/release/ita"
}
run_public() { # DIR ART_DIR -> echoes the gate transcript
  ( cd "$1" && PUBLIC_CORPORA_FILE="$1/scripts/corpora.txt" \
      PUBLIC_BASELINE_DIR="$1/scripts/baseline" \
      PUBLIC_CORPORA_ROOT="$c4/corpora" ART="$2" \
      bash "$1/scripts/public-gate.sh" 2>&1 )
}
if ! mutate "$ROOT/scripts/public-gate.sh" "$LAB/public-gate-mutant.sh" \
      'mkdir -p "$ART" || { printf ' \
      ': || { printf '; then
  bad 'public-gate-artifacts: mutation did not apply (INVALIDO-cmp)'
else
  # ... and neuter the freshness guard itself, not one of its three call
  # sites: the historical gate had no guard at all, and a mutant that
  # keeps two of the three assertions fails for the RIGHT reason and so
  # proves nothing about the wrong one.
  mutate "$LAB/public-gate-mutant.sh" "$LAB/public-gate-mutant.sh" \
    $'fresh_artifact() {\n  local path=$1 what=$2' \
    $'fresh_artifact() {\n  return 0 # mutant: freshness guard removed\n  local path=$1 what=$2' \
    || bad 'public-gate-artifacts: second mutation did not apply (INVALIDO-cmp)'
  bash -n "$LAB/public-gate-mutant.sh" || bad 'public-gate-artifacts: mutant does not parse (INVALIDO-parse)'
  mk_public "$c4/shipped" "$ROOT/scripts/public-gate.sh"
  mk_public "$c4/mutant" "$LAB/public-gate-mutant.sh"

  # Leg 1 — the defect: artifact dir absent, so nothing can be written.
  mut_out=$(run_public "$c4/mutant" "$c4/mutant/absent-art")
  if [[ $mut_out == *'match baseline exactly'* && ! -f $c4/mutant/absent-art/public-lab-fresh.txt ]]; then
    ok 'mutant reproduces the defect (agreement reported with no artifact on disk)'
  else
    bad 'mutant did NOT reproduce the defect — this case proves nothing'
  fi

  # Leg 2a — shipped, same condition: it creates the dir and really measures.
  ship_out=$(run_public "$c4/shipped" "$c4/shipped/absent-art")
  if [[ $ship_out == *'match baseline exactly'* && -s $c4/shipped/absent-art/public-lab-fresh.txt ]]; then
    ok 'shipped script creates the artifact dir and compares a real set'
  else
    bad "shipped script did not produce its artifacts: $ship_out"
  fi

  # Leg 2b — shipped, dir present but unwritable: FAIL naming the artifact.
  ro=$c4/shipped/readonly-art
  mkdir -p "$ro" && chmod 500 "$ro"
  ro_out=$(run_public "$c4/shipped" "$ro")
  chmod 700 "$ro"
  if [[ $ro_out == *'artifact never written'* && $ro_out == *'RESULT: FAIL'* ]]; then
    ok 'shipped script FAILS loudly when an artifact cannot be written'
  else
    bad "shipped script did not fail on an unwritable artifact dir: $ro_out"
  fi
fi

say 'summary'
if (( failed )); then echo "RESULT: FAIL (lab kept at $LAB)"; exit 1; fi
echo "RESULT: PASS (lab kept at $LAB)"
