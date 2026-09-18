#!/usr/bin/env bash
# replay.sh — judges a checker on mined bug/fix pairs, two-sidedly.
#
#   ./scripts/bug-replay/replay.sh [--repos rails,mastodon,discourse] \
#       [--limit N] [--srb-limit N] [--skip-srb] [--cleanup-worktrees]
#
# For each mined record (candidates/<id>.jsonl, newest first) it materializes
# the buggy PARENT revision and the FIX revision as detached git worktrees
# (~/Sites/temp-files/bug-replay-worktrees/<id>/<sha>, reused when present,
# never deleted unless --cleanup-worktrees is passed and every record using
# that sha is done), runs the freshly built release binary on both, and lets
# judge.rb classify:
#
#   HIT        diagnostic on a touched file at parent, gone at fix
#   MISS       silent on both (a false negative — acceptable, Invariant #1)
#   NOISE      diagnostic at fix the parent did not have — POTENTIAL FALSE
#              POSITIVE ON FIXED CODE; printed loudly, this is the failure
#              mode this project exists to avoid
#   UNRELATED  diagnostic present at parent, unchanged at fix
#   HIT?       a HIT whose silenced diagnostic's family does not match the
#              commit subject's failure vocabulary (class_match=false) —
#              counted separately in the summary; the label itself stays HIT
#
# When `srb` is on PATH and the checked-out fix revision has a sorbet/
# directory, `srb tc --typed true --no-color` runs on both revisions too
# (best effort, timeout 600s each) for the head-to-head.
#
# Exit codes, gauntlet-gates.sh style:
#   0  at least one repo judged
#   1  a hard failure (build failed)
#   2  nothing judged — every repo skipped (clone or candidates absent),
#      each named on its own SKIP line
set -uo pipefail

cd "$(dirname "$0")/../.."
ROOT=$PWD
HERE=scripts/bug-replay
CORPORA=${BUG_REPLAY_CORPORA:-$HOME/Sites/temp-files/bug-replay-corpora}
WTROOT=${BUG_REPLAY_WORKTREES:-$HOME/Sites/temp-files/bug-replay-worktrees}
RESULTS=$ROOT/$HERE/results
SRB_TIMEOUT=600

repos='rails,mastodon,discourse'
limit=40
srb_limit=''
skip_srb=0
cleanup=0
while [[ $# -gt 0 ]]; do
  case $1 in
    --repos) repos=$2; shift 2 ;;
    --limit) limit=$2; shift 2 ;;
    --srb-limit) srb_limit=$2; shift 2 ;;
    --skip-srb) skip_srb=1; shift ;;
    --cleanup-worktrees) cleanup=1; shift ;;
    *) echo "replay: unknown arg: $1 (see header)"; exit 2 ;;
  esac
done
SRB_LIMIT=${srb_limit:-$limit}
# One directory per invocation: results are never appended across runs
# (a 2026-09-03 MISS must not read as today's result). The manifest pins
# what produced them: source sha/dirtiness + binary sha256 — the evidence
# rule (captures prove the binary) in machine-checkable form.
RUN_ID=$(date +%Y%m%d-%H%M%S)-run
RUN_DIR=$RESULTS/$RUN_ID
mkdir -p "$RUN_DIR"
SOURCE_SHA=$(git rev-parse HEAD)
SOURCE_DIRTY=$(git status --porcelain | head -c 200)
RUN_MANIFEST=$RUN_DIR/manifest.json

write_manifest() { # called at exit; tools counted via local counters
  ruby -rjson -rdigest -e '
    sha, dirty, binary = ARGV
    bin_sha = File.exist?(binary) ? Digest::SHA256.file(binary).hexdigest : nil
    File.write(ARGV[4], JSON.generate(
      "run_id" => ARGV[3], "source_sha" => sha, "source_dirty" => dirty.empty? ? nil : dirty,
      "binary" => binary, "binary_sha256" => bin_sha, "completed" => ARGV[5]))' \
    "$SOURCE_SHA" "$SOURCE_DIRTY" "$ITA" "$RUN_ID" "$RUN_MANIFEST" "$1"
}
trap 'write_manifest no' EXIT


say() { printf '\n=== %s\n' "$1"; }
ok() { printf 'PASS %s\n' "$1"; }
bad() { printf 'FAIL %s\n' "$1"; }
skip() { printf 'SKIP %s — %s\n' "$1" "$2"; }

say 'build — cargo build --release (the evidence rule: captures prove the binary)'
if cargo build --release --locked --target-dir "$ROOT/target" >"$RUN_DIR/build.txt" 2>&1; then
  ok 'cargo build --release'
else
  bad "cargo build --release (see $RUN_DIR/build.txt)"
  exit 1
fi
ITA=$ROOT/target/release/ita
trap 'write_manifest yes' EXIT
HAVE_SRB=0
command -v srb >/dev/null && [[ $skip_srb -eq 0 ]] && HAVE_SRB=1

# ensure_wt ID SHA -> echoes the worktree dir, or fails
ensure_wt() {
  local wt="$WTROOT/$1/$2" avail
  [[ -f $wt/.git ]] && { echo "$wt"; return 0; }
  # disk guard: a worktree add materializes git objects; under the floor it
  # must abort loudly, not fill the last GiBs (this machine crashed that way once)
  avail=$(df -g /System/Volumes/Data 2>/dev/null | awk 'NR==2 {print $4}')
  if [[ -z $avail || $avail -lt 6 ]]; then
    printf 'replay: ABORT — %s GiB free on /System/Volumes/Data (floor: 6 GiB); refusing to create worktree %s\n' \
      "${avail:-unknown}" "$wt" >&2
    return 3
  fi
  git -C "$CORPORA/$1" worktree add --detach "$wt" "$2" >/dev/null 2>&1 \
    && { echo "$wt"; return 0; }
  return 1
}

# run_ita / run_srb ID SHA WT -> set REPLY_PATH (output file) and REPLY_RC.
# Cached per sha: consecutive records often share revisions.
run_ita() {
  local out="$RUN_DIR/$1/diag/ita-$2.jsonl"
  if [[ ! -f $out ]]; then
    "$ITA" check --format=json "$3" >"$out" 2>"$RUN_DIR/$1/diag/ita-$2.err"
    REPLY_RC=$?
  else
    REPLY_RC=0
  fi
  REPLY_PATH=$out
}
run_srb() {
  local out="$RUN_DIR/$1/diag/srb-$2.txt"
  if [[ ! -f $out ]]; then
    ( cd "$3" && timeout "$SRB_TIMEOUT" srb tc --typed true --no-color ) \
      >"$out" 2>"$RUN_DIR/$1/diag/srb-$2.err"
    REPLY_RC=$?
  else
    REPLY_RC=0
  fi
  REPLY_PATH=$out
}

# TSV summary of one candidate line: fix \t parent \t signal \t subject
read_fields() {
  ruby -rjson -e '
    r = JSON.parse(STDIN.read)
    puts [r["fix_sha"], r["parent_sha"], r["signal"],
          r["subject"].gsub(/[[:cntrl:]]/, " ")].join("\t")'
}

# judge.rb wrapper: ID RECFILE TOOL PARENT_DIAGS PARENT_ROOT FIX_DIAGS FIX_ROOT
judge_pair() {
  ruby "$ROOT/$HERE/judge.rb" \
    --record="$2" --parent-diags="$4" --parent-root="$5" \
    --fix-diags="$6" --fix-root="$7" --tool="$3"
}

overall=0
declare -A T  # T["repo|tool|label"]=count

for id in ${repos//,/ }; do
  clone=$CORPORA/$id
  cands=$ROOT/$HERE/candidates/$id.jsonl
  if [[ ! -d $clone/.git ]]; then
    skip "$id" "no clone at $clone — git clone --filter=blob:none https://github.com/<org>/$id first; nothing replayed for it"
    continue
  fi
  if [[ ! -s $cands ]]; then
    skip "$id" "no candidates at $cands — run: ruby $HERE/mine.rb $clone $id"
    continue
  fi

  say "$id — replaying up to $limit candidates (srb: $([[ $HAVE_SRB -eq 1 ]] && echo on || echo off))"
  mkdir -p "$RUN_DIR/$id/diag" "$RUN_DIR/$id/records"
  declare -A DIA SRB WTN
  start=$SECONDS

  # worktree refcounts over the selected slice (for --cleanup-worktrees)
  while IFS= read -r line; do
    IFS=$'\t' read -r fix parent _rest <<< "$(read_fields <<<"$line")"
    WTN[$fix]=$(( ${WTN[$fix]:-0} + 1 ))
    WTN[$parent]=$(( ${WTN[$parent]:-0} + 1 ))
  done < <(head -n "$limit" "$cands")

  n=0 judged=0 errs=0
  while IFS= read -r line; do
    n=$((n + 1))
    IFS=$'\t' read -r fix parent signal subject <<< "$(read_fields <<<"$line")"
    recfile=$RUN_DIR/$id/records/$n.json
    printf '%s\n' "$line" >"$recfile"

    wtp=$(ensure_wt "$id" "$parent") \
      || { bad "$id #$n: worktree add failed for parent $parent"; errs=$((errs+1)); continue; }
    wtf=$(ensure_wt "$id" "$fix") \
      || { bad "$id #$n: worktree add failed for fix $fix"; errs=$((errs+1)); continue; }

    # --- ita at both revisions ---
    ita_out=''
    run_ita "$id" "$parent" "$wtp"; DIA[$parent]=$REPLY_PATH
    prc=$REPLY_RC
    run_ita "$id" "$fix" "$wtf"; DIA[$fix]=$REPLY_PATH
    frc=$REPLY_RC
    if [[ $prc -gt 1 || $frc -gt 1 ]]; then
      bad "$id #$n: ita check failed (parent rc=$prc fix rc=$frc)"
      errs=$((errs+1))
    else
      if out=$(judge_pair "$id" "$recfile" ita "${DIA[$parent]}" "$wtp" "${DIA[$fix]}" "$wtf"); then
        verdict=$(ruby -rjson -e 'j = JSON.parse(STDIN.read); puts "#{j["label"]}\t#{j["class_match"]}"' <<<"$out")
        ilabel=${verdict%%$'\t'*}
        key=$ilabel
        if [[ $ilabel == HIT && ${verdict##*$'\t'} == false ]]; then key='HIT?'; fi
        T["$id|ita|$key"]=$(( ${T["$id|ita|$key"]:-0} + 1 ))
        judged=$((judged+1))
        ita_out=$out
        if [[ $key == NOISE ]]; then
          first_noise=$(ruby -rjson -e '
            j = JSON.parse(STDIN.read)
            d = j["files"].flat_map { |f| f["new"] }.first
            p = j["files"][0] && j["files"][0]["path"]
            puts d ? "#{d["key"]} at #{p}:#{d["line"]}" : "new diagnostic"' <<<"$out")
          printf '*** NOISE %s #%d "%s"\n    fix=%s signal=%s\n    %s\n' \
            "$id" "$n" "$subject" "$fix" "$signal" "$first_noise"
        fi
        if [[ $key == HIT? ]]; then
          printf '*** HIT? %s #%d "%s"\n    fix=%s signal=%s — silenced diagnostic outside the subject failure family (class_match=false)\n' \
            "$id" "$n" "$subject" "$fix" "$signal"
        fi
      else
        bad "$id #$n: judge.rb failed on ita output"
        errs=$((errs+1))
      fi
    fi

    # --- srb at both revisions, best effort ---
    srb_out=''
    if [[ $HAVE_SRB -eq 1 && $n -le $SRB_LIMIT && -d $wtf/sorbet ]]; then
      if [[ -n ${SRB[$parent]:-} ]]; then REPLY_RC=0; else
        run_srb "$id" "$parent" "$wtp"; SRB[$parent]=$REPLY_PATH
      fi
      prc2=$REPLY_RC
      if [[ -n ${SRB[$fix]:-} ]]; then REPLY_RC=0; else
        run_srb "$id" "$fix" "$wtf"; SRB[$fix]=$REPLY_PATH
      fi
      frc2=$REPLY_RC
      if [[ $prc2 -eq 124 || $frc2 -eq 124 ]]; then
        srb_status='timeout'
      elif [[ $prc2 -gt 1 || $frc2 -gt 1 ]]; then
        srb_status="error rc=$prc2/$frc2"
      else
        if out=$(judge_pair "$id" "$recfile" srb "${SRB[$parent]}" "" "${SRB[$fix]}" ""); then
          slabel=$(ruby -rjson -e 'puts JSON.parse(STDIN.read)["label"]' <<<"$out")
          T["$id|srb|$slabel"]=$(( ${T["$id|srb|$slabel"]:-0} + 1 ))
          srb_out=$out
        else
          bad "$id #$n: judge.rb failed on srb output"
          srb_status='error judge'
        fi
      fi
    fi

    # --- one result line per record, carrying both verdicts ---
    ruby -rjson -e '
      rec = JSON.parse(File.read(ARGV[0]))
      rec["ita"]  = ARGV[1].empty? ? {"label" => "ERROR"} : JSON.parse(ARGV[1])
      rec["srb"]  = ARGV[2].empty? ? {"label" => "SKIPPED"} : JSON.parse(ARGV[2])
      rec["srb_status"] = ARGV[3]
      puts JSON.generate(rec)' \
      "$recfile" "${ita_out}" "${srb_out}" "${srb_status:-}" >>"$RUN_DIR/$id.jsonl"

    if [[ $cleanup -eq 1 ]]; then
      for sha in "$fix" "$parent"; do
        WTN[$sha]=$(( ${WTN[$sha]} - 1 ))
        if [[ ${WTN[$sha]} -eq 0 ]]; then
          git -C "$clone" worktree remove --force "$WTROOT/$id/$sha" >/dev/null 2>&1 || true
        fi
      done
    fi
  done < <(head -n "$limit" "$cands")

  secs=$((SECONDS - start))
  printf 'replayed %s: %d records judged, %d errors, %ds\n' "$id" "$judged" "$errs" "$secs"
  [[ $judged -gt 0 || $errs -gt 0 ]] && overall=1
  unset DIA SRB WTN
done

say 'summary — HIT/MISS/NOISE/UNRELATED/HIT? per repo per tool'
printf '%-12s %-5s %7s %5s %5s %7s %9s %6s\n' repo tool records HIT MISS NOISE UNRELATED 'HIT?'
for id in ${repos//,/ }; do
  for tool in ita srb; do
    hit=${T["$id|$tool|HIT"]:-0} miss=${T["$id|$tool|MISS"]:-0}
    noise=${T["$id|$tool|NOISE"]:-0} unl=${T["$id|$tool|UNRELATED"]:-0}
    hitq=${T["$id|$tool|HIT?"]:-0}
    total=$(( hit + miss + noise + unl + hitq ))
    [[ $total -gt 0 ]] || continue
    printf '%-12s %-5s %7d %5d %5d %7d %9d %6d\n' "$id" "$tool" "$total" "$hit" "$miss" "$noise" "$unl" "$hitq"
  done
done
printf '\nresults: %s/<id>.jsonl (one line per record: ita + srb verdicts), diag/ under it holds the raw checker output\n' "$RUN_DIR"

if [[ $overall -eq 0 ]]; then
  echo 'RESULT: SKIP (nothing judged — clones/candidates absent, named above)'
  exit 2
fi
echo 'RESULT: PASS'
