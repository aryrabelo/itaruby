#!/usr/bin/env bash
# Two-sided proof for scripts/gate-digest (AGENTS.md, Rules of proof: the
# instrument that PRODUCES the evidence gets the same treatment as the code it
# judges, and the probes live next to the instrument).
#
# Silence side: on four fixture gauntlet directories under
# scripts/gate-digest-fixture/ the digest must report exactly the verdict the
# fixture encodes — all-green green, perf red, public-corpus drift with one new
# line, and a PASS whose evidence file is missing (which must read FAIL
# "artifact absent", never PASS — the public-gate false-green shape).
#
# Accusation side: each load-bearing decision inside gate-digest is removed by
# ONE `sed` on a COPY and the case that defends it must flip. Every mutation is
# `cmp`-guarded — a sed that matched nothing prints INVALIDO-cmp and fails the
# run, because a mutant that changed no bytes proves nothing (AGENTS.md,
# measured 2026-09-17). And for every mutant the `green` case must stay green:
# a mutant that breaks every case is a broken harness wearing a verdict, not a
# demonstration that the named case is the one watching.
#
#   0  both sides proved
#   1  a fixture verdict is wrong, or a mutant was not accused by its case
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1
ROOT=$PWD
DIGEST=$ROOT/scripts/gate-digest
FIX=$ROOT/scripts/gate-digest-fixture
LAB=${GATE_DIGEST_LAB:-$HOME/Sites/temp-files/gate-digest-selftest-$(date +%Y%m%d-%H%M%S)}

case $LAB in
  /*) ;;
  *) printf 'FAIL GATE_DIGEST_LAB must be an ABSOLUTE path, got %q\n' "$LAB" >&2; exit 1 ;;
esac
case $LAB in
  "$ROOT"|"$ROOT"/|"$HOME"|/) printf 'FAIL GATE_DIGEST_LAB refuses %q as a scratch lab\n' "$LAB" >&2; exit 1 ;;
esac

failed=0
ok()  { printf 'PASS %s\n' "$1"; }
bad() { printf 'FAIL %s\n' "$1"; failed=1; }
say() { printf '\n=== %s\n' "$1"; }

rm -rf "$LAB"
mkdir -p "$LAB" || exit 1

# Build case <name> as a gauntlet artifact dir: the green fixture, then the
# case's overlay on top, minus whatever its REMOVE list names.
build_case() {
  local name=$1 dir="$LAB/art-$1"
  rm -rf "$dir"
  mkdir -p "$dir"
  cp "$FIX"/green/* "$dir"/
  if [[ $name != green ]]; then
    for f in "$FIX/$name"/*; do
      [[ -e $f ]] || continue
      [[ $(basename "$f") == REMOVE ]] && continue
      cp "$f" "$dir"/
    done
    if [[ -f "$FIX/$name/REMOVE" ]]; then
      while read -r victim; do
        [[ -z $victim ]] && continue
        rm -f "$dir/$victim"
      done <"$FIX/$name/REMOVE"
    fi
  fi
  printf '%s\n' "$dir"
}

# Run a digest binary over a case; echo the digest.json path.
run_case() {
  local bin=$1 name=$2 dir
  dir=$(build_case "$name")
  "$bin" --art "$dir" --transcript "$dir/transcript.txt" --out "$dir/digest.json" >/dev/null 2>"$dir/stderr.txt"
  printf '%s\n' "$dir/digest.json"
}

# A case's verdict, as a boolean over the parsed digest. `expr` is Ruby with
# `d` bound to the digest hash and `g` to a lambda finding a gate by name.
holds() {
  local json=$1 expr=$2
  ruby -rjson -e '
    d = JSON.parse(File.read(ARGV[0]))
    g = ->(n) { d["gates"].find { |x| x["name"] == n } || {} }
    exit(eval(ARGV[1]) ? 0 : 1)
  ' "$json" "$expr" 2>/dev/null
}

# The four fixture claims, one per case. Named so a mutant can point at one.
claim_green='d["result"] == "PASS" && d["counts"]["fail"] == 0 && g.("a")["numbers"]["passed"] == 16 && d["machine"]["load1"] == 2.71'
claim_perf='g.("c3")["status"] == "FAIL" && g.("c3")["numbers"]["over"] == 1 && g.("c3")["evidence"]["artifact"] == "perf.txt" && !g.("c3")["evidence"]["lines"].empty?'
claim_public='g.("f")["status"] == "FAIL" && g.("f")["numbers"]["repos"][0]["new"] == 1 && g.("f")["numbers"]["repos"][0]["new_at"] == ["public-rails-new.txt:1"] && g.("f")["evidence"]["excerpt"].join.include?("casted.rb")'
claim_absent='g.("a")["status"] == "FAIL" && g.("a")["reason"].include?("artifact absent") && g.("a")["reason"].include?("tests.txt")'
claim_secrecy='g.("d")["status"] == "FAIL" && g.("d")["numbers"]["corpora"][0]["new"] == 1 && g.("d")["evidence"]["excerpt_withheld"].to_s.include?("secrecy") && !File.read(ARGV[0]).include?("SEKRIT_FIXTURE_TOKEN")'

case_expr() {
  case $1 in
    green)        printf '%s' "$claim_green" ;;
    perf-fail)    printf '%s' "$claim_perf" ;;
    public-drift) printf '%s' "$claim_public" ;;
    absent)       printf '%s' "$claim_absent" ;;
    secrecy)      printf '%s' "$claim_secrecy" ;;
  esac
}

CASES=(green perf-fail public-drift absent secrecy)

say 'silence side — the shipped digest reports each fixture exactly'
for c in "${CASES[@]}"; do
  j=$(run_case "$DIGEST" "$c")
  if holds "$j" "$(case_expr "$c")"; then
    ok "case $c reported correctly ($(wc -c <"$j" | tr -d ' ') bytes)"
  else
    bad "case $c misreported (see $j)"
  fi
done

# Reading the digest must be free, which is the whole point. Two ceilings,
# both measured here rather than invented: an all-green digest is verdicts and
# numbers only (1791B measured), a red one additionally carries the failing
# reason plus one excerpt/line-ref block (2010-2263B measured). Slack is debt
# (AGENTS.md): tighten these when the measured numbers drop.
for c in "${CASES[@]}"; do
  j="$LAB/art-$c/digest.json"
  size=$(wc -c <"$j" | tr -d ' ')
  if [[ $c == green ]]; then ceiling=2048; else ceiling=2560; fi
  if (( size <= ceiling )); then
    ok "case $c digest is ${size}B (<= $ceiling)"
  else
    bad "case $c digest is ${size}B > $ceiling"
  fi
done

say 'accusation side — each decision removed, its own case must flip'
# name|case that must flip|sed program
mutants=(
  'absence_is_agreement|absent|s/unless absent.empty?/if absent.empty? \&\& false/'
  'secrecy_wall_off|secrecy|s/^def private_artifact?(path)$/def private_artifact?(_path)\n  return false/'
  'perf_over_blind|perf-fail|s/.over. => m\[5\] != .ok./'"'"'over'"'"' => false/'
  'public_drift_counts_dropped|public-drift|s/r\[kind\] = lines.length/r[kind] = 0/'
  'fail_verdict_ignored|perf-fail|s/if !fails.empty? then .FAIL./if !fails.empty? then '"'"'PASS'"'"'/'
)
for entry in "${mutants[@]}"; do
  IFS='|' read -r mname mcase mprog <<<"$entry"
  mbin="$LAB/gate-digest.$mname"
  sed "$mprog" "$DIGEST" >"$mbin"
  chmod +x "$mbin"
  if cmp -s "$DIGEST" "$mbin"; then
    bad "INVALIDO-cmp: mutant $mname changed no bytes — the sed matched nothing, so nothing was proved"
    continue
  fi
  j=$(run_case "$mbin" "$mcase")
  if holds "$j" "$(case_expr "$mcase")"; then
    bad "mutant $mname NOT accused: case $mcase still reports correctly"
  else
    # Positive control: the mutation must not break the harness wholesale.
    jg=$(run_case "$mbin" green)
    if holds "$jg" "$claim_green"; then
      ok "mutant $mname accused by case $mcase (green case still green)"
    else
      bad "mutant $mname breaks case green too — broken mutation, not a demonstration"
    fi
  fi
done

say 'summary'
if (( failed )); then echo "RESULT: FAIL (lab kept at $LAB)"; exit 1; fi
rm -rf "$LAB"
echo 'RESULT: PASS (both sides proved)'
