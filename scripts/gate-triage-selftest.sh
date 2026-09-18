#!/usr/bin/env bash
# Two-sided proof for scripts/gate-triage (AGENTS.md, Rules of proof: the
# probes live next to the instrument, and an advisory router is still an
# instrument — a triage that routes a red run to the wrong action costs the
# lead a wasted repair, and one that routes a green run to action costs a
# false alarm).
#
# Silence side: on eight fixture gauntlet runs (the five gate-digest fixtures
# plus rev-drift, loaded and skipped overlays) the triage must route EXACTLY
# the actions the fixture encodes — and the green run must route NOTHING.
# A recorded-answers leg replays the 2026-09-18 measured Jev answers for the
# perf-fail shape, so the answer-refinement path is proved offline too.
#
# Accusation side: each load-bearing decision inside gate-triage is removed
# by ONE `sed` on a COPY and the case that defends it must flip. Every
# mutation is `cmp`-guarded — a sed that matched nothing prints
# INVALIDO-cmp and fails the run, because a mutant that changed no bytes
# proves nothing (AGENTS.md, measured 2026-09-17). For every mutant the
# `green` case must stay green: a mutant that breaks every case is a broken
# harness wearing a verdict, not a demonstration that the named case is the
# one watching.
#
# Also proved here, because the instrument promises them:
#   - state is a pure function of digest.json (the secrecy wall travels with
#     the digest: mutate a corpus artifact AFTER the digest is written and
#     the state must not move);
#   - answers are validated before anything is sent (unknown key = exit 2);
#   - transport failure is fail-open (exit 4 + "triage unavailable", the
#     gauntlet verdict stands);
#   - reading the state stays free (a byte ceiling on the green state,
#     measured here, tightened when the measured number drops).
#
# Offline by construction: no network request carries a fixture byte. The
# one live transport test aims at a closed local port.
#
#   0  both sides proved
#   1  a fixture routed wrong, or a mutant was not accused by its case
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1
ROOT=$PWD
DIGEST_BIN=$ROOT/scripts/gate-digest
TRIAGE=$ROOT/scripts/gate-triage
GFIX=$ROOT/scripts/gate-digest-fixture
TFIX=$ROOT/scripts/gate-triage-fixture
LAB=${GATE_TRIAGE_LAB:-$HOME/Sites/temp-files/gate-triage-selftest-$(date +%Y%m%d-%H%M%S)}

case $LAB in
  /*) ;;
  *) printf 'FAIL GATE_TRIAGE_LAB must be an ABSOLUTE path, got %q\n' "$LAB" >&2; exit 1 ;;
esac
case $LAB in
  "$ROOT"|"$ROOT"/|"$HOME"|/) printf 'FAIL GATE_TRIAGE_LAB refuses %q as a scratch lab\n' "$LAB" >&2; exit 1 ;;
esac

failed=0
ok()  { printf 'PASS %s\n' "$1"; }
bad() { printf 'FAIL %s\n' "$1"; failed=1; }
say() { printf '\n=== %s\n' "$1"; }

rm -rf "$LAB"
mkdir -p "$LAB" || exit 1

# Build case <name> as a gauntlet artifact dir: the green fixture base, then
# overlays from either fixture root (gate-digest-fixture for the digest's own
# cases, gate-triage-fixture for the triage's extras).
build_case() {
  local name=$1 dir="$LAB/art-$1" src f
  rm -rf "$dir"
  mkdir -p "$dir"
  cp "$GFIX"/green/* "$dir"/
  if [[ $name != green ]]; then
    for src in "$GFIX/$name" "$TFIX/$name"; do
      [[ -d $src ]] || continue
      for f in "$src"/*; do
        [[ -e $f ]] || continue
        [[ $(basename "$f") == REMOVE ]] && continue
        cp "$f" "$dir"/
      done
      if [[ -f "$src/REMOVE" ]]; then
        while read -r victim; do
          [[ -z $victim ]] && continue
          rm -f "$dir/$victim"
        done <"$src/REMOVE"
      fi
    done
  fi
  printf '%s\n' "$dir"
}

# Run the shipped digest over a case; echo the digest.json path.
digest_case() {
  local name=$1 dir
  dir=$(build_case "$name")
  "$DIGEST_BIN" --art "$dir" --transcript "$dir/transcript.txt" --out "$dir/digest.json" >/dev/null 2>"$dir/digest.err"
  printf '%s\n' "$dir/digest.json"
}

# Run a triage binary over a case; echo the triage.json path.
triage_case() {
  local bin=$1 name=$2 answers=${3:-} j dir
  j=$(digest_case "$name")
  dir=$(dirname "$j")
  if [[ -n $answers ]]; then
    "$bin" classify --digest "$j" --answers "$answers" >"$dir/triage.json" 2>"$dir/triage.err"
  else
    "$bin" classify --digest "$j" >"$dir/triage.json" 2>"$dir/triage.err"
  fi
  printf '%s\n' "$dir/triage.json"
}

# A case's routing, as a boolean over the parsed triage. `expr` is Ruby with
# `t` bound to the triage hash and `ids` to the action ids.
holds_t() {
  local json=$1 expr=$2
  ruby -rjson -e '
    t = JSON.parse(File.read(ARGV[0]))
    ids = t["actions"].map { |a| a["id"] }
    exit(eval(ARGV[1]) ? 0 : 1)
  ' "$json" "$expr" 2>/dev/null
}

# The eight fixture claims, one per case. Named so a mutant can point at one.
claim_green='ids.empty? && t["escalate"] == false && t["recommended_next"].nil?'
claim_perf='ids.include?("perf_over_without_parent_comparison") && !ids.include?("measured_under_load")'
claim_perf_ans='ids.include?("perf_over_without_parent_comparison") && ids.include?("jev_no_parent_measured") && ids.include?("jev_root_cause") && t["recommended_next"] == "re-medir" && t["escalate"] == false'
claim_public='ids.include?("public_drift_unaudited")'
claim_absent='ids.include?("artifact_absent")'
claim_secrecy='ids.include?("corpus_error_drift") && !ids.include?("corpus_rev_drift")'
claim_revdrift='ids.include?("corpus_rev_drift") && !ids.include?("corpus_error_drift")'
claim_loaded='ids == ["measured_under_load"] && t["digest_result"].to_s.include?("PASS")'
claim_skipped='ids.include?("corpus_skipped_here") && !ids.include?("corpus_error_drift")'

case_expr() {
  case $1 in
    green)        printf '%s' "$claim_green" ;;
    perf-fail)    printf '%s' "$claim_perf" ;;
    public-drift) printf '%s' "$claim_public" ;;
    absent)       printf '%s' "$claim_absent" ;;
    secrecy)      printf '%s' "$claim_secrecy" ;;
    rev-drift)    printf '%s' "$claim_revdrift" ;;
    loaded)       printf '%s' "$claim_loaded" ;;
    skipped)      printf '%s' "$claim_skipped" ;;
  esac
}

CASES=(green perf-fail public-drift absent secrecy rev-drift loaded skipped)

say 'silence side — the shipped triage routes each fixture exactly'
for c in "${CASES[@]}"; do
  j=$(triage_case "$TRIAGE" "$c")
  if holds_t "$j" "$(case_expr "$c")"; then
    ok "case $c routed correctly"
  else
    bad "case $c routed wrong (see $j)"
  fi
done

say 'silence side — recorded Jev answers refine, never flip, the routing'
j=$(triage_case "$TRIAGE" perf-fail "$TFIX/answers/perf-fail.json")
if holds_t "$j" "$claim_perf_ans"; then
  ok "perf-fail + recorded answers routes re-medir without escalating"
else
  bad "perf-fail + recorded answers routed wrong (see $j)"
fi

say 'economy side — reading the state stays free (ceiling measured here)'
j=$(digest_case green)
sdir=$(dirname "$j")
"$TRIAGE" state --digest "$j" >"$sdir/state.json" 2>/dev/null
size=$(wc -c <"$sdir/state.json" | tr -d ' ')
if (( size <= 2600 )); then
  ok "green state is ${size}B <= 2600B ceiling"
else
  bad "green state is ${size}B > 2600B ceiling"
fi

say 'secrecy side — state is a pure function of digest.json'
j=$(digest_case green)
pdir=$(dirname "$j")
"$TRIAGE" state --digest "$j" >"$pdir/s1.json" 2>/dev/null
printf 'SEKRIT_FIXTURE_TOKEN gone=zzz new=yyy\n' >>"$pdir/corpus-corpus-a.txt"
"$TRIAGE" state --digest "$j" >"$pdir/s2.json" 2>/dev/null
if cmp -s "$pdir/s1.json" "$pdir/s2.json" && ! grep -q SEKRIT_FIXTURE_TOKEN "$pdir/s2.json"; then
  ok "state unmoved by artifact mutation, token never leaves (secrecy wall holds)"
else
  bad "state moved with artifact content — the secrecy wall has a bypass"
fi

say 'input side — answers are validated before anything is sent'
j=$(digest_case green)
"$TRIAGE" classify --digest "$j" --answers "$TFIX/answers/bad.json" >/dev/null 2>&1
code=$?
if (( code == 2 )); then
  ok "unknown answer key rejected (exit 2)"
else
  bad "unknown answer key accepted (exit $code, wanted 2)"
fi

say 'transport side — failure is fail-open (the gauntlet verdict stands)'
j=$(digest_case green)
out=$(env -u OPENROUTER_API_KEY GATE_TRIAGE_ENDPOINT=http://127.0.0.1:9 "$TRIAGE" run --digest "$j" 2>&1)
code=$?
if [[ $code -eq 4 ]] && printf '%s' "$out" | grep -q 'triage unavailable'; then
  ok "transport failure routed fail-open (exit 4 + advice line)"
else
  bad "transport failure exited $code (wanted 4 + 'triage unavailable')"
fi

say 'accusation side — each decision removed, its own case must flip'
# name|guard|sed program; guard is case:<name>, exit2, exit4 or purity
mutants=(
  'perf_rule_off|case:perf-fail|s/perf_over_without_parent_comparison/PERF_RULE_OFF/g'
  'corpus_error_drift_off|case:secrecy|s/corpus_error_drift/CORPUS_ERR_OFF/g'
  'rev_drift_off|case:rev-drift|s/corpus_rev_drift/REV_OFF/g'
  'skip_blind|case:skipped|s/corpus_skipped_here/SKIP_OFF/g'
  'absence_agreement|case:absent|s/artifact absent/ARTIFACT_ABSENT_OFF/g'
  'public_blind|case:public-drift|s/public_drift_unaudited/PUB_OFF/g'
  'load_blind|case:loaded|s/load1 > 8\.0/load1 > 999.0/'
  'answers_unvalidated|exit2|s/unknown = ans.keys - ALLOWED_ANSWERS/unknown = []/'
  'transport_fail_closed|exit4|s/EXIT_TRANSPORT = 4/EXIT_TRANSPORT = 1/'
  'state_leaks_artifacts|purity|s/prune_digest(digest)/prune_digest(digest).merge("leak" => File.read(File.join(File.dirname(digest_path), "corpus-corpus-a.txt")))/'
)
green_ctrl() { # bin -> 0 when the green case still routes empty
  local jg
  jg=$(triage_case "$1" green)
  holds_t "$jg" "$claim_green"
}
for entry in "${mutants[@]}"; do
  IFS='|' read -r mname mguard mprog <<<"$entry"
  mbin="$LAB/gate-triage.$mname"
  sed "$mprog" "$TRIAGE" >"$mbin"
  chmod +x "$mbin"
  if cmp -s "$TRIAGE" "$mbin"; then
    bad "INVALIDO-cmp: mutant $mname changed no bytes — the sed matched nothing, so nothing was proved"
    continue
  fi
  accused=0
  case $mguard in
    case:*)
      mcase=${mguard#case:}
      jm=$(triage_case "$mbin" "$mcase")
      holds_t "$jm" "$(case_expr "$mcase")" || accused=1
      ;;
    exit2)
      jx=$(digest_case green)
      "$mbin" classify --digest "$jx" --answers "$TFIX/answers/bad.json" >/dev/null 2>&1
      [[ $? -ne 2 ]] && accused=1
      ;;
    exit4)
      jx=$(digest_case green)
      env -u OPENROUTER_API_KEY GATE_TRIAGE_ENDPOINT=http://127.0.0.1:9 \
        "$mbin" run --digest "$jx" >/dev/null 2>&1
      [[ $? -ne 4 ]] && accused=1
      ;;
    purity)
      jx=$(digest_case green)
      pdir=$(dirname "$jx")
      "$mbin" state --digest "$jx" >"$pdir/m1.json" 2>/dev/null
      printf 'SEKRIT_FIXTURE_TOKEN gone=zzz new=yyy\n' >>"$pdir/corpus-corpus-a.txt"
      "$mbin" state --digest "$jx" >"$pdir/m2.json" 2>/dev/null
      # The mutant is accused when the silence breaks: state moved with the
      # artifact, or the token itself reached the state.
      if ! cmp -s "$pdir/m1.json" "$pdir/m2.json" || grep -q SEKRIT_FIXTURE_TOKEN "$pdir/m2.json"; then
        accused=1
      fi
      ;;
  esac
  if (( accused )); then
    if green_ctrl "$mbin"; then
      ok "mutant $mname accused by its guard ($mguard; green case still green)"
    else
      bad "mutant $mname breaks case green too — broken mutation, not a demonstration"
    fi
  else
    bad "mutant $mname NOT accused: its guard ($mguard) still passes"
  fi
done

say 'summary'
if (( failed )); then echo "RESULT: FAIL (lab kept at $LAB)"; exit 1; fi
rm -rf "$LAB"
echo 'RESULT: PASS (both sides proved)'
