#!/usr/bin/env bash
# Two-sided proof for scripts/public-drift-attrib (AGENTS.md, Rules of proof:
# the instrument that classifies the evidence gets the same treatment as the
# code it judges, and the probes live next to the instrument).
#
# Silence side: on seven fixture pairs under
# scripts/public-drift-attrib-fixture/ the classifier must split new/gone into
# moved lines vs real drift EXACTLY as the fixture encodes — an all-shift
# re-pin reports zero real drift, a genuine new/gone finding is counted, a
# same-message-different-file pair is NOT a shift, a same-file-different-
# message pair is NOT a shift, and an extra occurrence of a key is one shift
# plus one real. Each case is also cross-checked against `comm`: the
# classifier's new/gone totals must equal the set difference `comm` computes
# over the same sorted inputs, so the split can never be a fiction over the
# wrong totals. An eighth leg proves the hard stop: a non-JSON line makes the
# classifier EXIT 2, never silently drop the line it cannot read (the
# false-green shape this repo already paid for).
#
# Accusation side: each load-bearing decision inside the classifier is removed
# by ONE `sed` on a COPY and the case that defends it must flip. Every
# mutation is `cmp`-guarded — a sed that matched nothing prints INVALIDO-cmp
# and fails the run, because a mutant that changed no bytes proves nothing
# (AGENTS.md, measured 2026-09-17). For every mutant the `identical` case must
# stay correct: a mutant that breaks every case is a broken harness wearing a
# verdict, not a demonstration that the named case is the one watching.
#
#   0  both sides proved
#   1  a fixture was misclassified, a comm cross-check disagreed, or a mutant
#      was not accused by its case
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1
ROOT=$PWD
ATTRIB=$ROOT/scripts/public-drift-attrib
FIX=$ROOT/scripts/public-drift-attrib-fixture
LAB=${PUBLIC_DRIFT_LAB:-$HOME/Sites/temp-files/public-drift-attrib-selftest-$(date +%Y%m%d-%H%M%S)}

case $LAB in
  /*) ;;
  *) printf 'FAIL PUBLIC_DRIFT_LAB must be an ABSOLUTE path, got %q\n' "$LAB" >&2; exit 1 ;;
esac
case $LAB in
  "$ROOT"|"$ROOT"/|"$HOME"|/) printf 'FAIL PUBLIC_DRIFT_LAB refuses %q as a scratch lab\n' "$LAB" >&2; exit 1 ;;
esac

failed=0
ok()  { printf 'PASS %s\n' "$1"; }
bad() { printf 'FAIL %s\n' "$1"; failed=1; }
say() { printf '\n=== %s\n' "$1"; }

rm -rf "$LAB"
mkdir -p "$LAB" || exit 1

# Derive the gate's own new/gone sets for a fixture: filter to JSON diagnostic
# lines, sort-unique, then the two `comm` halves — byte-for-byte the pipeline
# scripts/public-gate.sh runs. Echoes "<new_count> <gone_count>" (the comm
# oracle) and leaves $LAB/<case>.{new,gone} on disk.
derive() {
  local name=$1 dir="$LAB/$1"
  mkdir -p "$dir"
  grep -E '^\{' "$FIX/$name/fresh.jsonl" | sort -u >"$dir/fresh"
  grep -E '^\{' "$FIX/$name/baseline.jsonl" | sort -u >"$dir/base"
  comm -23 "$dir/fresh" "$dir/base" >"$dir/new"
  comm -13 "$dir/fresh" "$dir/base" >"$dir/gone"
  printf '%s %s\n' "$(wc -l <"$dir/new" | tr -d ' ')" "$(wc -l <"$dir/gone" | tr -d ' ')"
}

# Run a classifier binary over a derived case; echo the summary.json path.
run_case() {
  local bin=$1 name=$2
  local dir="$LAB/$name"
  "$bin" --id "$name" --new "$dir/new" --gone "$dir/gone" --json >"$dir/summary.json" 2>"$dir/stderr.txt"
  printf '%s\n' "$dir/summary.json"
}

# A case's verdict, as a boolean over the parsed summary. `expr` is Ruby with
# `a` bound to the summary hash. `cn`/`cg` are the comm oracle counts, so a
# claim can pin the classifier's totals to `comm`'s.
holds() {
  local json=$1 expr=$2 cn=$3 cg=$4
  ruby -rjson -e '
    a = JSON.parse(File.read(ARGV[0]))
    cn = ARGV[2].to_i
    cg = ARGV[3].to_i
    exit(eval(ARGV[1]) ? 0 : 1)
  ' "$json" "$expr" "$cn" "$cg" 2>/dev/null
}

# The seven fixture claims, one per case. Each pins the classification AND the
# comm cross-check (a["new"]==cn && a["gone"]==cg). Named so a mutant points
# at one.
claim_identical='a["new"]==cn && a["gone"]==cg && a["new"]==0 && a["gone"]==0 && a["moved"]==0 && a["new_real"]==0 && a["gone_real"]==0'
claim_shift='a["new"]==cn && a["gone"]==cg && a["new"]==3 && a["gone"]==3 && a["moved"]==3 && a["new_real"]==0 && a["gone_real"]==0 && a["shifts"].map { |s| s["from"] }.sort == [12,30,40]'
claim_new='a["new"]==cn && a["gone"]==cg && a["new"]==1 && a["gone"]==0 && a["moved"]==0 && a["new_real"]==1 && a["gone_real"]==0'
claim_gone='a["new"]==cn && a["gone"]==cg && a["new"]==0 && a["gone"]==1 && a["moved"]==0 && a["new_real"]==0 && a["gone_real"]==1'
claim_path='a["new"]==cn && a["gone"]==cg && a["new"]==1 && a["gone"]==1 && a["moved"]==0 && a["new_real"]==1 && a["gone_real"]==1'
claim_message='a["new"]==cn && a["gone"]==cg && a["new"]==1 && a["gone"]==1 && a["moved"]==0 && a["new_real"]==1 && a["gone_real"]==1'
claim_count='a["new"]==cn && a["gone"]==cg && a["new"]==2 && a["gone"]==1 && a["moved"]==1 && a["new_real"]==1 && a["gone_real"]==0'

case_expr() {
  case $1 in
    identical)      printf '%s' "$claim_identical" ;;
    shift-only)     printf '%s' "$claim_shift" ;;
    new-diagnostic) printf '%s' "$claim_new" ;;
    gone-diagnostic) printf '%s' "$claim_gone" ;;
    path-crossed)   printf '%s' "$claim_path" ;;
    message-change) printf '%s' "$claim_message" ;;
    count-change)   printf '%s' "$claim_count" ;;
  esac
}

CASES=(identical shift-only new-diagnostic gone-diagnostic path-crossed message-change count-change)

say 'silence side — the shipped classifier splits each fixture exactly (and agrees with comm)'
for c in "${CASES[@]}"; do
  read -r cn cg < <(derive "$c")
  j=$(run_case "$ATTRIB" "$c")
  if holds "$j" "$(case_expr "$c")" "$cn" "$cg"; then
    ok "case $c classified correctly (comm: $cn new, $cg gone)"
  else
    bad "case $c misclassified (see $j; comm said $cn new, $cg gone)"
  fi
done

# The hard stop: a line that is not a JSON diagnostic object must EXIT 2, not
# be dropped. The fixture feeds raw new/gone (as if run on raw `ita` output),
# so the gate's `^\{` filter is deliberately not in the way.
malformed_holds() { # bin -> 0 when it refuses the malformed line loudly
  local bin=$1
  "$bin" --id malformed --new "$FIX/malformed/new.txt" --gone "$FIX/malformed/gone.txt" \
    >/dev/null 2>"$LAB/malformed.stderr"
  local code=$?
  [[ $code -eq 2 ]] && grep -q 'not a JSON diagnostic object' "$LAB/malformed.stderr"
}

say 'silence side — a non-JSON line is a hard stop, never a dropped line'
if malformed_holds "$ATTRIB"; then
  ok 'malformed input exits 2 naming the unreadable line'
else
  bad "malformed input was not refused loudly (see $LAB/malformed.stderr)"
fi

say 'accusation side — each decision removed, its own case must flip'
# name|case that must flip|sed program
mutants=(
  'never_pairs|shift-only|s/pairs = \[n\.length, g\.length\]\.min/pairs = 0/'
  'key_drops_path|path-crossed|s/KEY_FIELDS = %w\[code message path\]/KEY_FIELDS = %w[code message]/'
  'key_drops_message|message-change|s/KEY_FIELDS = %w\[code message path\]/KEY_FIELDS = %w[code path]/'
  'real_new_dropped|new-diagnostic|s/real_new.concat(n\[pairs..\] || \[\])/real_new.concat([])/'
  'real_gone_dropped|gone-diagnostic|s/real_gone.concat(g\[pairs..\] || \[\])/real_gone.concat([])/'
  'malformed_accepted|__malformed__|s|.*is not a JSON diagnostic object.*|    obj = {} unless obj.is_a?(Hash)|'
)
for entry in "${mutants[@]}"; do
  IFS='|' read -r mname mcase mprog <<<"$entry"
  mbin="$LAB/public-drift-attrib.$mname"
  sed "$mprog" "$ATTRIB" >"$mbin"
  chmod +x "$mbin"
  if cmp -s "$ATTRIB" "$mbin"; then
    bad "INVALIDO-cmp: mutant $mname changed no bytes — the sed matched nothing, so nothing was proved"
    continue
  fi
  ruby -c "$mbin" >/dev/null 2>&1 || { bad "INVALIDO-parse: mutant $mname does not parse"; continue; }

  if [[ $mcase == __malformed__ ]]; then
    accused=1; malformed_holds "$mbin" && accused=0
  else
    read -r cn cg < <(derive "$mcase")
    j=$(run_case "$mbin" "$mcase")
    accused=1; holds "$j" "$(case_expr "$mcase")" "$cn" "$cg" && accused=0
  fi

  if (( accused == 0 )); then
    bad "mutant $mname NOT accused: case $mcase still passes"
    continue
  fi
  # Positive control: the mutation must not break the harness wholesale.
  read -r icn icg < <(derive identical)
  jg=$(run_case "$mbin" identical)
  if holds "$jg" "$claim_identical" "$icn" "$icg"; then
    ok "mutant $mname accused by case $mcase (identical case still correct)"
  else
    bad "mutant $mname breaks case identical too — broken mutation, not a demonstration"
  fi
done

say 'summary'
if (( failed )); then echo "RESULT: FAIL (lab kept at $LAB)"; exit 1; fi
rm -rf "$LAB"
echo 'RESULT: PASS (both sides proved)'
