#!/usr/bin/env bash
# Public-corpus gate. The launch bar for this project is beating Sorbet on
# speed and true errors on four PUBLIC Rails apps (rails/rails, mastodon,
# discourse, gitlab-foss — see AGENTS.md's "benchmarked against it"). Today
# that measurement is ad hoc; this gate makes it an exit code.
#
#   0  every declared repo cloneable + at baseline, under its time ceiling
#   1  a repo's diagnostic set drifted (new/gone lines) or exceeded its
#      time ceiling
#   2  gates green, but at least one declared repo could not be judged on
#      this machine — a missing/bad clone (auto-clone is OFF by default), a
#      missing baseline, or a broken ita binary — the summary names which
#
# Repos are declared one per line in scripts/public-corpora.txt:
#   <id> <git-url> <pinned-sha> <dev_ceiling_s>[:<ci_ceiling_s>] [<clone-dir>]
# (ids: rails, mastodon, discourse, gitlab-foss — public code, so no
# secrecy wall here; the diagnostic baseline for each id is stored IN THE
# CLEAR in scripts/public-baseline/<id>.jsonl, normalized by stripping each
# repo's clone root and replacing it with `<id>/` so the baseline is
# machine-independent).
#
# <clone-dir> is OPTIONAL and names the directory under
# $PUBLIC_CORPORA_ROOT that holds the pinned revision; without it the
# canonical `<id>` clone is used and the historical semantics stand (a
# missing or wrong-HEAD clone is a SKIP, or a repair under
# PUBLIC_GATE_CLONE=1). WITH it the declaration is a claim about which tree
# is measured, and the gate is FAIL-CLOSED over that claim: the path must
# exist and its HEAD must be the declared sha, else the repo FAILS — never
# skipped, never measured. A gate that measures one revision while its
# transcript talks about another is the false-green shape this repo already
# paid for twice (AGENTS.md), and it is how a re-pin silently becomes a
# measurement of the revision it meant to replace.
#
# Every judged repo prints `public revision (<id>): tree <path> at sha <sha>`
# before anything is compared, so no number in the transcript (or in
# scripts/gate-digest's read of it) can belong to an unnamed tree.
#
# Unlike the private corpora (which the project never writes), the public
# corpora DO get cloned on demand — but never automatically. gitlab-foss
# alone is multi-GB, so the human decides when a machine is ready: only
# PUBLIC_GATE_CLONE=1 turns a missing clone into a clone, or a
# broken/pinned-mismatched clone into a repair. Without it that repo is a
# per-repo SKIP (exit 2), named in the transcript.
#
# Clones live at $PUBLIC_CORPORA_ROOT/<id> (default
# $HOME/Sites/temp-files/public-corpora), or at
# $PUBLIC_CORPORA_ROOT/<clone-dir> when the declaration names one — a linked
# `git worktree` of the canonical clone is the normal case for a re-pin,
# because re-pointing the canonical clone would move the tree another
# measurement may still be reading. Baselines live at
# scripts/public-baseline/<id>.jsonl unless PUBLIC_BASELINE_DIR redirects
# the directory (a test hook; the committed layout is authoritative).
#
# Sorbet timing is informational only: when `srb` is on PATH and the clone
# carries a `sorbet/` directory, `srb tc --typed true` is timed (best
# effort, 600s cap) and printed next to ita's wall time. It never affects
# the exit code. A missing srb prints "srb: not installed".
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1
ROOT=$PWD
ITA=$ROOT/target/release/ita
CORPORA_FILE=${PUBLIC_CORPORA_FILE:-$ROOT/scripts/public-corpora.txt}
BASELINE_DIR=${PUBLIC_BASELINE_DIR:-$ROOT/scripts/public-baseline}
CLONE_ROOT=${PUBLIC_CORPORA_ROOT:-$HOME/Sites/temp-files/public-corpora}
ATTRIB=${PUBLIC_ATTRIB:-$ROOT/scripts/public-drift-attrib}
ART=${ART:-$ROOT/target/gauntlet}
SRB_TIMEOUT=600
# Like PERF_COLUMN, this is explicit: a local shell exporting CI must not
# silently trade its measured dev ceiling for a hosted-runner ceiling.
COLUMN=${PUBLIC_COLUMN:-dev}
if [[ $COLUMN != dev && $COLUMN != ci ]]; then
  printf 'FAIL public corpus: PUBLIC_COLUMN must be dev or ci, got %q\n' "$COLUMN"; exit 1
fi
printf 'public gate: column=%s\n' "$COLUMN"

# The artifact directory MUST exist before anything writes into it. Until
# 2026-09-17 it did not: on a fresh tree (or after `rm -rf target/`),
# every `>"$ART/..."` redirection failed, `ita check`'s output went
# nowhere, and the comparison that follows compared two EMPTY files — so
# the gate printed "public errors match baseline exactly" for all three
# repos in 0.02s while measuring nothing, and exited 0. Measured on this
# gate, the same false-green shape AGENTS.md records for
# `gauntlet-gates.sh` and `replay.sh`: a gate that reads evidence it
# never made.
mkdir -p "$ART" || { printf 'FAIL public corpus: cannot create %s\n' "$ART"; exit 1; }

# "Written THIS run" is proved by deletion, not by a timestamp: each
# iteration removes the artifacts it is about to write (`clear_artifacts`)
# and then requires them to exist and be non-empty (`fresh_artifact`).
# Timestamps were the first design and were rejected — `[[ f -nt g ]]`
# compares whole seconds on this platform, so a gate that finishes inside
# the same second as its own sentinel would fail for no reason. Deletion
# has no clock in it.
clear_artifacts() {
  rm -f "$@"
}

# Exists and is non-empty, or a FAIL naming the artifact — never a silent
# PASS. `ita check` on a real corpus always emits at least one diagnostic
# line, so empty here means "the redirection went nowhere" or "the binary
# produced nothing", both of which used to read as agreement.
fresh_artifact() {
  local path=$1 what=$2
  if [[ ! -f $path ]]; then
    bad "$what: artifact never written ($path)"
    return 1
  fi
  if [[ ! -s $path ]]; then
    bad "$what: artifact is empty ($path)"
    return 1
  fi
  return 0
}

failed=0
skipped=0
public_skipped=()
say() { printf '\n=== %s\n' "$1"; }
ok()   { printf 'PASS %s\n' "$1"; }
bad()  { printf 'FAIL %s\n' "$1"; failed=1; }
skip() { printf 'SKIP %s — %s\n' "$1" "$2"; public_skipped+=("$1"); skipped=1; }

if command -v srb >/dev/null 2>&1; then SRB_PRESENT=1; else SRB_PRESENT=0; fi

if [[ ! -x $ITA ]]; then
  bad 'public corpus: no target/release/ita'
fi

if [[ ! -f $CORPORA_FILE ]]; then
  bad "public corpus: $CORPORA_FILE missing; a gate with no declared repos proves nothing"
fi

while read -r id url sha ceiling clone_dir; do
  [[ -z $id || $id == \#* ]] && continue
  if [[ ! $sha =~ ^[0-9a-f]{40}$ ]]; then
    bad "public corpus: malformed repo line (bad pinned sha) in $CORPORA_FILE: $id"
    continue
  fi
  base="$BASELINE_DIR/$id.jsonl"
  clone="$CLONE_ROOT/${clone_dir:-$id}"

  # A repo with no baseline cannot be judged — that is a SKIP, not a
  # failure: baselines are generated once a machine actually holds a clone.
  # Leaving the baseline absent keeps the repo skipped by id.
  if [[ ! -s $base ]]; then
    skip "public corpus ($id)" "no baseline $base; leave it absent to keep this repo skipped until a machine generates one"
    continue
  fi

  # A declared repo may not be cloneable on this machine. AUTO-CLONE IS OFF
  # by default: only PUBLIC_GATE_CLONE=1 creates a missing clone or repairs
  # a broken/pinned-mismatched one. Otherwise the repo is skipped, never
  # failed — a missing clone is a machine limitation, not a regression.
  #
  # An EXPLICIT <clone-dir> changes that: the declaration names the tree that
  # will be measured, so a missing path or a wrong HEAD is a FAIL, not a
  # SKIP, and PUBLIC_GATE_CLONE does not rescue it either — a "repair" that
  # silently checks out the declared sha into a tree someone else may be
  # reading is exactly the surprise this rule exists to prevent.
  if [[ -n $clone_dir && ! -d $clone ]]; then
    bad "public corpus ($id): declared clone $clone is missing — refusing to measure another tree (fail-closed)"
    continue
  fi
  if [[ ! -d $clone ]]; then
    if [[ ${PUBLIC_GATE_CLONE:-0} == 1 ]]; then
      mkdir -p "$CLONE_ROOT"
      if ! git clone --quiet "$url" "$clone" >"$ART/public-$id.txt" 2>&1; then
        bad "public corpus ($id): clone failed (see target/gauntlet/public-$id.txt)"
        continue
      fi
    else
      skip "public corpus ($id)" "no clone at $clone (set PUBLIC_GATE_CLONE=1 to clone $url)"
      continue
    fi
  fi
  head=$(git -C "$clone" rev-parse HEAD 2>/dev/null || true)
  if [[ -n $clone_dir && $head != "$sha" ]]; then
    bad "public corpus ($id): declared clone $clone is at ${head:-<no HEAD>}, not the declared sha $sha — refusing to measure another revision (fail-closed)"
    continue
  fi
  if [[ $head != "$sha" ]]; then
    if [[ ${PUBLIC_GATE_CLONE:-0} == 1 ]]; then
      if ! git -C "$clone" fetch --quiet --depth 1 origin "$sha" >"$ART/public-$id.txt" 2>&1 &&
         ! git -C "$clone" fetch --quiet origin "$sha" >"$ART/public-$id.txt" 2>&1; then
        bad "public corpus ($id): could not fetch pinned sha $sha (see target/gauntlet/public-$id.txt)"
        continue
      fi
      git -C "$clone" checkout --quiet --detach "$sha" >"$ART/public-$id.txt" 2>&1 || {
        bad "public corpus ($id): could not checkout pinned sha $sha"
        continue
      }
    else
      skip "public corpus ($id)" "clone HEAD ($head) != pinned $sha (set PUBLIC_GATE_CLONE=1 to re-pin)"
      continue
    fi
  fi
  # Name the measured tree before any number is printed. Every sha the gate
  # utters from here on belongs to this path.
  ok "public revision ($id): tree $clone at sha $head"

  out="$ART/public-$id.txt"
  clear_artifacts "$out" "$ART/public-$id-fresh.txt" "$ART/public-$id-expected.txt" \
    "$ART/public-$id-new.txt" "$ART/public-$id-gone.txt" "$ART/public-$id-drift.txt"
  start=$(python3 -c 'import time; print(time.time())')
  "$ITA" check "$clone" --format=json >"$out" 2>&1
  end=$(python3 -c 'import time; print(time.time())')
  elapsed=$(python3 -c "print(f'{$end - $start:.3f}')")

  # Normalization: strip the clone root from each JSON `path` field and
  # replace it with `<id>/`, so the baseline depends on neither the machine
  # nor the checkout location — only on the id and the file's path relative
  # to the repo root. Only JSON diagnostic lines (starting with `{`) join the
  # comparison: `ita check` also prints discovery announcements to stderr
  # ("itaruby: discovered ..."), which `2>&1` captures for the transcript but
  # which are not diagnostics and must never enter the set. The SORTED
  # diagnostic set (unique lines) is the comparison unit, matching the
  # private corpus baseline's set semantics.
  sed "s|\"path\":\"$clone/|\"path\":\"$id/|g" "$out" | grep -E '^\{' | sort -u >"$ART/public-$id-fresh.txt"
  grep -E '^\{' "$base" | sort -u >"$ART/public-$id-expected.txt"

  # Both sides of the comparison must be real files this run produced.
  # Before this check, a missing $ART made every redirection above fail and
  # `comm` compared nothing against nothing — reported as an exact match.
  fresh_artifact "$out" "public corpus ($id)" || continue
  fresh_artifact "$ART/public-$id-fresh.txt" "public corpus ($id) fresh set" || continue
  fresh_artifact "$ART/public-$id-expected.txt" "public corpus ($id) baseline set" || continue

  comm -23 "$ART/public-$id-fresh.txt" "$ART/public-$id-expected.txt" >"$ART/public-$id-new.txt"
  comm -13 "$ART/public-$id-fresh.txt" "$ART/public-$id-expected.txt" >"$ART/public-$id-gone.txt"
  new=$(wc -l <"$ART/public-$id-new.txt" | tr -d ' ')
  gone=$(wc -l <"$ART/public-$id-gone.txt" | tr -d ' ')
  lines=$(wc -l <"$ART/public-$id-fresh.txt" | tr -d ' ')
  errs=$(grep -c '"severity":"error"' "$ART/public-$id-fresh.txt" || true)
  warns=$(( lines - errs ))
  base_lines=$(wc -l <"$ART/public-$id-expected.txt" | tr -d ' ')
  base_errs=$(grep -c '"severity":"error"' "$ART/public-$id-expected.txt" || true)
  base_warns=$(( base_lines - base_errs ))

  if (( new == 0 && gone == 0 )); then
    ok "public errors match baseline exactly ($id): $errs error(s) + $warns warning(s) = $lines line(s), tree $clone at sha $head"
  else
    # The set changed, so the run is red either way; what this adds is WHICH
    # KIND of change it is. Most of a re-pin's "drift" is files moving under
    # the diagnostics — the same path+code+message with only `line` changed,
    # which is not a detection change at all. Before
    # scripts/public-drift-attrib that split was done by hand (2026-09-19:
    # rails showed 180 new / 169 gone lines, of which 168 were pure shifts).
    drift="$ART/public-$id-drift.txt"
    if [[ -x $ATTRIB ]] && attrib_json=$("$ATTRIB" --id "$id" \
          --new "$ART/public-$id-new.txt" --gone "$ART/public-$id-gone.txt" \
          --tree "$clone" --rev "$head" --report "$drift" --json \
          2>"$ART/public-$id-attrib-stderr.txt"); then
      moved=$(printf '%s' "$attrib_json" | python3 -c 'import json,sys; print(json.load(sys.stdin)["moved"])' 2>/dev/null || true)
      bad "public errors drifted ($id): $new new, $gone gone — ${moved:-?} line(s) are pure line shifts, the rest is real drift ($errs error(s) now vs $base_errs in baseline; see target/gauntlet/public-$id-drift.txt)"
    else
      {
        echo "# drift for public corpus $id — in the clear (public repo, no secrecy wall)"
        echo "# measured tree $clone at $head"
        echo "## new (present now, not in baseline; $new line(s)):"
        cat "$ART/public-$id-new.txt"
        echo "## gone (in baseline, not produced now; $gone line(s)):"
        cat "$ART/public-$id-gone.txt"
      } >"$drift"
      bad "public errors drifted ($id): $new new, $gone gone — drift attribution UNAVAILABLE ($ATTRIB did not run; raw sets, unclassified, in target/gauntlet/public-$id-drift.txt)"
    fi
  fi

  # Wall-time ceiling. Slack is debt (AGENTS.md, binding): the ceiling is a
  # real measured number per machine, set to measured time x1.5 (rounded up)
  # when the baseline was anchored — never invented. A missing CI number
  # fails closed: another machine's column is not a fallback measurement.
  dev_ceiling=${ceiling%%:*}
  ci_ceiling=
  [[ $ceiling == *:* ]] && ci_ceiling=${ceiling#*:}
  case $COLUMN in
    dev) limit=$dev_ceiling ;;
    ci)  limit=$ci_ceiling ;;
  esac
  if [[ -z $limit ]]; then
    bad "public time ceiling ($id): no $COLUMN ceiling measured (declare <dev>:<ci> in $CORPORA_FILE; never copy the other column)"
  elif [[ ! $limit =~ ^[0-9]+([.][0-9]+)?$ ]]; then
    bad "public time ceiling ($id): invalid $COLUMN ceiling $limit"
  elif python3 -c 'import sys; sys.exit(0 if float(sys.argv[1]) <= float(sys.argv[2]) else 1)' "$elapsed" "$limit"; then
    ok "public time ceiling ($id): ${elapsed}s <= ${limit}s"
  else
    bad "public time ceiling ($id): ${elapsed}s > ${limit}s (or comparison failed)"
  fi

  if (( SRB_PRESENT )); then
    if [[ -d $clone/sorbet ]]; then
      st=$(python3 -c 'import time; print(time.time())')
      timeout "$SRB_TIMEOUT" srb tc --typed true >"$ART/public-$id-srb.txt" 2>&1
      en=$(python3 -c 'import time; print(time.time())')
      selapsed=$(python3 -c "print(f'{$en - $st:.3f}')")
      printf '  srb tc --typed true: %ss (informational)\n' "$selapsed"
    fi
  else
    printf '  srb: not installed\n'
  fi
done < <(grep -vE '^\s*(#|$)' "$CORPORA_FILE")

say 'summary'
(( ${#public_skipped[@]} )) && echo "public corpus gate skipped (not judged on this machine): ${public_skipped[*]}"
if (( failed )); then echo 'RESULT: FAIL'; exit 1; fi
if (( skipped )); then echo "RESULT: PASS (incomplete — skipped: ${public_skipped[*]})"; exit 2; fi
echo 'RESULT: PASS'