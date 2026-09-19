#!/usr/bin/env bash
# Two-sided proof for the inference bench itself.
#
# AGENTS.md, binding (2026-09-17): "the instrument that PRODUCES the evidence
# gets the same two-sided treatment as the code it judges". The bench decides
# whether itaruby finds real bugs; nothing decided whether the bench can tell.
# So each way the bench could lie is re-injected as a mutant here, and the
# bench must accuse the RIGHT case for the RIGHT reason.
#
# Three guards, because a mutation harness lies more easily than the thing it
# tests (AGENTS.md: "a control that fails everywhere is a broken fixture
# wearing a passing verdict"):
#
#   G1 pristine control — the unmutated lab must PASS first. If it does not,
#      the lab is broken and every later "caught" is meaningless; abort.
#   G2 named accusation — each mutant must make the bench fail AND name the
#      case it was injected into AND match the expected reason. A failure
#      somewhere else is not a catch.
#   G3 blast radius — each mutant must leave every OTHER case still OK. A
#      mutant that turns the whole board red proves nothing.
#
# The lab is built under $ROOT/target/ on purpose: the bench derives its own
# root from `__dir__/..`, so a lab outside the repo silently judges the real
# tree instead of the mutated copy (the 2026-09-17 positive-control trap).
set -uo pipefail

cd "$(dirname "$0")/.."
ROOT=$PWD
ITA=$ROOT/target/release/ita
LAB=${INFERENCE_SELFTEST_LAB:-$ROOT/target/inference-selftest}
CASES=testdata/gauntlet_inference

fail=0
say()  { printf '\n--- %s\n' "$1"; }
ok()   { printf '  PASS %s\n' "$1"; }
bad()  { printf '  FAIL %s\n' "$1"; fail=1; }

if [[ ! -x $ITA ]]; then
  echo 'SKIP inference bench selftest — no target/release/ita (build first)'
  exit 2
fi
have_srb=0
command -v srb >/dev/null && have_srb=1

rebuild_lab() {
  rm -rf "$LAB"
  mkdir -p "$LAB/scripts" "$LAB/testdata"
  cp "$ROOT/scripts/inference-bench.rb" "$LAB/scripts/"
  cp "$ROOT/scripts/inference-bench.jsonl" "$LAB/scripts/"
  cp -R "$ROOT/$CASES" "$LAB/testdata/gauntlet_inference"
}

# Runs the bench in the lab. Echoes its output; returns its exit code.
run_lab() {
  local extra=()
  [[ ${1:-} == sorbet ]] && extra+=(--sorbet)
  # `${extra[@]}` on an EMPTY array is an unbound-variable error under
  # `set -u` in bash < 4.4 — which is macOS's /bin/bash 3.2, and that is
  # what `#!/usr/bin/env bash` resolves to when the lab runs with a PATH
  # that has no newer bash ahead of it. Measured 2026-09-19 on gate g:
  # `line 57: extra[@]: unbound variable` aborted the selftest before its
  # first case, so the bench had no guard at all while the gate reported
  # FAIL for a reason that had nothing to do with the bench. The `+` form
  # expands to nothing when the array is empty, on every bash.
  ruby "$LAB/scripts/inference-bench.rb" --ita "$ITA" ${extra[@]+"${extra[@]}"} 2>&1
}

# G1 — the pristine control.
say 'G1 pristine control (the lab must pass before any mutation means anything)'
rebuild_lab
pristine=$(run_lab); pristine_rc=$?
if (( pristine_rc != 0 )); then
  echo "$pristine" | tail -20
  echo '  ABORT: the unmutated lab does not pass; every mutant verdict below would be noise'
  exit 1
fi
ok 'unmutated lab passes'

# G2 + G3 — one mutant at a time.
#   $1 label   $2 case the mutant lives in   $3 reason regex   $4 leg (plain|sorbet)
check_mutant() {
  local label=$1 target=$2 reason=$3 leg=${4:-plain}
  local out rc
  out=$(run_lab "$leg"); rc=$?
  if (( rc == 0 )); then
    bad "$label — bench still PASSED; it cannot see this defect"
    return
  fi
  if ! grep -qE "^ *${target}: .*${reason}" <<<"$out"; then
    bad "$label — bench failed, but not for the expected reason on ${target}"
    grep -E '^ +[a-z_]+: ' <<<"$out" | head -4 | sed 's/^/      got: /'
    return
  fi
  # G3: exactly one case may be red.
  local red
  red=$(grep -cE '^FAIL ' <<<"$out")
  if (( red != 1 )); then
    bad "$label — blast radius $red cases, expected exactly 1 (broken lab, not a catch)"
    grep -E '^FAIL ' <<<"$out" | sed 's/^/      /'
    return
  fi
  ok "$label"
}

say 'M1 the bug is silently fixed — an `accuse` fixture that no longer raises'
rebuild_lab
sed -i '' 's/total_cent$/total_cents/' "$LAB/$CASES/open_class_reopen_typo/no_annotations/plain.rb"
check_mutant 'M1 runtime no longer proves the bug' 'open_class_reopen_typo' 'MRI exited 0'

say 'M2 a `silent` fixture that actually raises — silence would be a false negative'
rebuild_lab
printf '\nSettings.new({}).nope\n' >>"$LAB/$CASES/define_method_loop/no_annotations/plain.rb"
check_mutant 'M2 clean-run claim is false' 'define_method_loop' 'MRI raised'

say 'M3 the fairness leg stops being the same program'
rebuild_lab
printf '\n# drifted\n' >>"$LAB/$CASES/method_missing_proxy/with_annotations/plain.rb"
check_mutant 'M3 fairness leg drift' 'method_missing_proxy' 'NOT the same program'

say 'M4 the pinned sigil is changed under the bench'
rebuild_lab
sed -i '' '1s/.*/# typed: false/' "$LAB/$CASES/attr_reader_typo/no_annotations/plain.rb"
check_mutant 'M4 sigil drift' 'attr_reader_typo' 'manifest pins'

say 'M5 an expected itaruby diagnostic line is wrong'
rebuild_lab
ruby -e '
  p = ARGV[0]
  rows = File.readlines(p).map { |l| JSON.parse(l) }
  rows.each { |r| r["ita"]["diags"].each { |d| d["line"] += 1 } if r["id"] == "namespace_reopen_typo" }
  File.write(p, rows.map { |r| JSON.generate(r) }.join("\n") + "\n")
' -rjson "$LAB/scripts/inference-bench.jsonl"
check_mutant 'M5 wrong expected line' 'namespace_reopen_typo' 'expected .*got'

say 'M6 a case directory with no manifest row'
rebuild_lab
cp -R "$LAB/$CASES/respond_to_guard" "$LAB/$CASES/unlisted_case"
out=$(run_lab); rc=$?
if (( rc != 0 )) && grep -qE 'coverage: case directories with no manifest row: .*unlisted_case' <<<"$out"; then
  ok 'M6 unlisted case directory'
else
  bad 'M6 unlisted case directory — bench did not notice an unjudged fixture'
  grep -E '^FAIL ' <<<"$out" | head -3 | sed 's/^/      got: /'
fi

say 'M10 the positive control stops raising — it controls for nothing'
rebuild_lab
# Repair the planted typo. MRI now exits 0, so the control can no longer
# tell "correctly silent" from "blind", and the bench must say so.
sed -i '' 's/config\.hostt/config.host/' "$LAB/$CASES/define_method_loop/positive_control/plain.rb"
check_mutant 'M10 positive control no longer raises' 'define_method_loop' 'controls for nothing'

say 'M11 a positive control directory the manifest does not declare'
rebuild_lab
mkdir -p "$LAB/$CASES/respond_to_guard/positive_control"
cp "$LAB/$CASES/respond_to_guard/no_annotations/plain.rb" \
   "$LAB/$CASES/respond_to_guard/positive_control/plain.rb"
check_mutant 'M11 undeclared positive control' 'respond_to_guard' 'manifest declares none'

say 'M12 the positive control silently starts being caught'
rebuild_lab
# The honest record today is `miss`. If itaruby ever catches it, the bench
# must NOT keep printing `miss` — that would be the overclaim in reverse.
ruby -e '
  p = ARGV[0]
  rows = File.readlines(p).map { |l| JSON.parse(l) }
  rows.each do |r|
    next unless r["id"] == "singleton_class_eval"
    r["positive_control"]["ita"]["diags"] = [{ "code" => "E0101", "line" => 11 }]
  end
  File.write(p, rows.map { |r| JSON.generate(r) }.join("\n") + "\n")
' -rjson "$LAB/scripts/inference-bench.jsonl"
check_mutant 'M12 positive-control verdict drift' 'singleton_class_eval' 'positive control: itaruby expected'

say 'M13 the binary crashes — silence must not read as a pass'
rebuild_lab
# A non-binary in the ita slot: no diagnostics on stdout, which is
# byte-identical to "clean" unless exit status and stderr are measured.
printf '#!/bin/sh\necho "boom" >&2\nexit 101\n' >"$LAB/fake-ita"
chmod +x "$LAB/fake-ita"
out=$(ruby "$LAB/scripts/inference-bench.rb" --ita "$LAB/fake-ita" 2>&1); rc=$?
if (( rc != 0 )) && grep -qE 'itaruby exited 101' <<<"$out"; then
  ok 'M13 crashing binary is caught instead of scoring as silence'
else
  bad "M13 crashing binary — expected failure naming the exit status, got rc=$rc"
  grep -E '^ +[a-z_]+: ' <<<"$out" | head -3 | sed 's/^/      got: /'
fi
if (( have_srb )); then
  say 'M7 the declaration the fairness leg depends on is removed'
  rebuild_lab
  rm "$LAB/$CASES/singleton_class_eval/with_annotations/shim.rbi"
  check_mutant 'M7 fairness claim without its declaration' 'singleton_class_eval' 'sorbet must be clean once declared' sorbet

  say 'M8 the pinned sorbet version no longer matches'
  rebuild_lab
  ruby -e '
    p = ARGV[0]
    rows = File.readlines(p).map { |l| JSON.parse(l) }
    rows.each { |r| r["sorbet_version"] = "0.0.0-not-this-one" }
    File.write(p, rows.map { |r| JSON.generate(r) }.join("\n") + "\n")
  ' -rjson "$LAB/scripts/inference-bench.jsonl"
  out=$(run_lab sorbet); rc=$?
  if (( rc == 2 )) && grep -q 'sorbet leg: srb' <<<"$out"; then
    ok 'M8 version drift skips the leg instead of re-measuring it'
  else
    bad "M8 version drift — expected exit 2 and a named skip, got rc=$rc"
  fi

  say 'M9 a second file inside a case directory — the isolation guard'
  rebuild_lab
  # A shared/polluted --dir is how one case silently reads another case's
  # results. The bench claims it verifies every diagnostic's path; this is
  # the mutant that makes it prove it.
  cat >"$LAB/$CASES/respond_to_guard/no_annotations/intruder.rb" <<'EOF'
# typed: true
class Intruder
  def call
    self.nope
  end
end
EOF
  check_mutant 'M9 diagnostic from another file in the case dir' 'respond_to_guard' 'reported outside the case' sorbet
else
  printf '\nSKIP M7/M8/M9 — no `srb` on this machine\n'
fi

say 'shipped scripts are clean (the mutants never touched the real tree)'
if git -C "$ROOT" diff --quiet -- scripts/inference-bench.rb scripts/inference-bench.jsonl "$CASES" 2>/dev/null; then
  ok 'no mutation leaked into the working tree'
else
  # Untracked-but-new files are the normal state while this bench is being
  # built; only a MODIFIED tracked file means a mutant escaped the lab.
  if [[ -z $(git -C "$ROOT" diff --name-only -- scripts/inference-bench.rb scripts/inference-bench.jsonl "$CASES") ]]; then
    ok 'no mutation leaked into tracked files'
  else
    bad 'a mutant modified the real tree'
  fi
fi

rm -rf "$LAB"
if (( fail )); then echo -e '\nRESULT: FAIL (inference bench selftest)'; exit 1; fi
echo -e '\nRESULT: PASS (inference bench selftest)'
