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
#   <id> <git-url> <pinned-sha> <time_ceiling_s>
# (ids: rails, mastodon, discourse, gitlab-foss — public code, so no
# secrecy wall here; the diagnostic baseline for each id is stored IN THE
# CLEAR in scripts/public-baseline/<id>.jsonl, normalized by stripping each
# repo's clone root and replacing it with `<id>/` so the baseline is
# machine-independent).
#
# Unlike the private corpora (which the project never writes), the public
# corpora DO get cloned on demand — but never automatically. gitlab-foss
# alone is multi-GB, so the human decides when a machine is ready: only
# PUBLIC_GATE_CLONE=1 turns a missing clone into a clone, or a
# broken/pinned-mismatched clone into a repair. Without it that repo is a
# per-repo SKIP (exit 2), named in the transcript.
#
# Clones live at $PUBLIC_CORPORA_ROOT/<id> (default
# $HOME/Sites/temp-files/public-corpora). Baselines live at
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
ART=${ART:-$ROOT/target/gauntlet}
SRB_TIMEOUT=600

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

while read -r id url sha ceiling; do
  [[ -z $id || $id == \#* ]] && continue
  if [[ ! $sha =~ ^[0-9a-f]{40}$ ]]; then
    bad "public corpus: malformed repo line (bad pinned sha) in $CORPORA_FILE: $id"
    continue
  fi
  base="$BASELINE_DIR/$id.jsonl"
  clone="$CLONE_ROOT/$id"

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

  out="$ART/public-$id.txt"
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
  comm -23 "$ART/public-$id-fresh.txt" "$ART/public-$id-expected.txt" >"$ART/public-$id-new.txt"
  comm -13 "$ART/public-$id-fresh.txt" "$ART/public-$id-expected.txt" >"$ART/public-$id-gone.txt"
  new=$(wc -l <"$ART/public-$id-new.txt" | tr -d ' ')
  gone=$(wc -l <"$ART/public-$id-gone.txt" | tr -d ' ')

  if (( new == 0 && gone == 0 )); then
    ok "public errors match baseline exactly ($id)"
  else
    drift="$ART/public-$id-drift.txt"
    {
      echo "# drift for public corpus $id — in the clear (public repo, no secrecy wall)"
      echo "## new (present now, not in baseline; $new line(s)):"
      cat "$ART/public-$id-new.txt"
      echo "## gone (in baseline, not produced now; $gone line(s)):"
      cat "$ART/public-$id-gone.txt"
    } >"$drift"
    bad "public errors drifted ($id): $new new, $gone gone (see target/gauntlet/public-$id-drift.txt)"
  fi

  # Wall-time ceiling. Slack is debt (AGENTS.md, binding): the ceiling is a
  # real measured number per machine, set to measured time x1.5 (rounded up)
  # when the baseline was anchored — never invented.
  if python3 -c "import sys; sys.exit(0 if $elapsed > $ceiling else 1)"; then
    bad "public time ceiling ($id): ${elapsed}s > ${ceiling}s"
  else
    ok "public time ceiling ($id): ${elapsed}s <= ${ceiling}s"
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