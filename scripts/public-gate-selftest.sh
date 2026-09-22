#!/usr/bin/env bash
# Two-sided proof for the FAIL-CLOSED clone-path check in
# scripts/public-gate.sh (AGENTS.md, Rules of proof: a gate that can read
# another tree than the one its transcript names is the false-green shape this
# repo already paid for twice, so the decision that forbids it gets probes
# next to it).
#
# The public gate takes an optional 5th declaration field, <clone-dir>, that
# names the exact tree to measure for a re-pin. When it is present the gate is
# fail-closed: the path must exist AND its HEAD must be the declared sha, else
# the repo FAILS — never a SKIP, never a silent measurement of the wrong
# revision. Without the field the historical semantics stand (a missing or
# wrong-HEAD canonical clone is a SKIP).
#
# Silence side: four labelled corpora over one real git clone —
#   match        declared tree at the declared sha  -> PASS, tree+sha named
#   wrong-rev    declared tree at a DIFFERENT sha    -> FAIL (fail-closed)
#   missing      declared tree absent               -> FAIL (fail-closed)
#   default-skip NO field, canonical clone off-pin  -> SKIP (unchanged)
#
# Accusation side: each half of the fail-closed guard is removed by one
# literal replacement on a COPY of the gate, and the case that defends it must
# flip from FAIL to SKIP. Every mutation is diff-guarded (a replacement that
# changed nothing is INVALIDO), and for every mutant the `match` case must
# stay PASS — a mutant that breaks the happy path is a broken harness, not a
# demonstration.
#
# Timing controls also select measured dev/CI columns independently of CI,
# refuse unmeasured/invalid CI ceilings, and mutate column selection. The
# workflow shell blocks run in a transport-stubbed lab: accepting other
# SKIPs/failures and fetching unbaselined corpora each have a named mutant.
#
#   0  both sides proved
#   1  a case behaved wrong, or a mutant was not accused by its case
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1
ROOT=$PWD
GATE=$ROOT/scripts/public-gate.sh
LAB=${PUBLIC_GATE_LAB:-$HOME/Sites/temp-files/public-gate-selftest-$(date +%Y%m%d-%H%M%S)}

case $LAB in
  /*) ;;
  *) printf 'FAIL PUBLIC_GATE_LAB must be an ABSOLUTE path, got %q\n' "$LAB" >&2; exit 1 ;;
esac
case $LAB in
  "$ROOT"|"$ROOT"/|"$HOME"|/) printf 'FAIL PUBLIC_GATE_LAB refuses %q as a scratch lab\n' "$LAB" >&2; exit 1 ;;
esac

failed=0
ok()  { printf 'PASS %s\n' "$1"; }
bad() { printf 'FAIL %s\n' "$1"; failed=1; }
say() { printf '\n=== %s\n' "$1"; }

rm -rf "$LAB"
mkdir -p "$LAB" || exit 1

# git subprocesses must never inherit a hook's GIT_DIR/GIT_INDEX_FILE: in a
# linked worktree those beat cwd and the probe would act on the real repo.
git_clean() { env -u GIT_DIR -u GIT_INDEX_FILE -u GIT_WORK_TREE -u GIT_PREFIX \
  -u GIT_COMMON_DIR -u GIT_AUTHOR_DATE -u GIT_COMMITTER_DATE git "$@"; }

# mutate FILE_IN FILE_OUT 'literal-needle' 'literal-replacement' (exit 3 = no-op)
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

CLONES=$LAB/corpora
mkdir -p "$CLONES"

# One real two-commit clone. HEAD (SGOOD) is the declared sha for the healthy
# cases; its parent (SPARENT) stands in for "the tree is at another revision".
HEADCLONE=$CLONES/lab-head
mkdir -p "$HEADCLONE"
git_clean init -q "$HEADCLONE"
git_clean -C "$HEADCLONE" config user.email probe@lab
git_clean -C "$HEADCLONE" config user.name probe
printf 'class L\n  def a\n    1\n  end\nend\n' >"$HEADCLONE/f.rb"
git_clean -C "$HEADCLONE" add -A && git_clean -C "$HEADCLONE" commit -qm one
SPARENT=$(git_clean -C "$HEADCLONE" rev-parse HEAD)
printf '# touched\n' >>"$HEADCLONE/f.rb"
git_clean -C "$HEADCLONE" add -A && git_clean -C "$HEADCLONE" commit -qm two
SGOOD=$(git_clean -C "$HEADCLONE" rev-parse HEAD)

# The canonical (no-field) clone for the default-skip control, deliberately
# off its declared pin.
CANON=$CLONES/lab
git_clean clone -q "$HEADCLONE" "$CANON"
git_clean -C "$CANON" checkout -q --detach "$SGOOD"

# A lab tree holding a COPY of the gate plus a fake ita and a baseline. The
# fake ita emits one diagnostic pathed inside whatever clone it is handed
# ($2), so the gate's own normalization rewrites it to `lab/f.rb` and the
# baseline matches on the measured case.
mk_lab() { # DIR GATE_SRC
  mkdir -p "$1/scripts/baseline" "$1/target/release"
  cp "$2" "$1/scripts/public-gate.sh"
  chmod +x "$1/scripts/public-gate.sh"
  printf '{"code":"E0101","column":5,"line":3,"message":"lab","path":"lab/f.rb","severity":"error"}\n' \
    >"$1/scripts/baseline/lab.jsonl"
  {
    echo '#!/bin/sh'
    echo '# fake ita: one diagnostic, pathed inside the clone it is handed ($2).'
    printf 'printf %s %s\n' \
      "'"'{"code":"E0101","column":5,"line":3,"message":"lab","path":"%s/f.rb","severity":"error"}\n'"'" \
      '"$2"'
  } >"$1/target/release/ita"
  chmod +x "$1/target/release/ita"
}

corpora_file() { # DIR NAME LINE
  printf '%s\n' "$3" >"$LAB/$2.corpora.txt"
  printf '%s\n' "$LAB/$2.corpora.txt"
}

COL=dev
run_gate() { # LABDIR CORPORA ART -> transcript on stdout, rc in $LAB/last-rc
  local column_env=(PUBLIC_COLUMN="$COL")
  [[ $COL == default ]] && column_env=(-u PUBLIC_COLUMN)
  ( cd "$1" && env "${column_env[@]}" CI=true PUBLIC_CORPORA_FILE="$2" PUBLIC_BASELINE_DIR="$1/scripts/baseline" \
      PUBLIC_CORPORA_ROOT="$CLONES" ART="$3" bash "$1/scripts/public-gate.sh" 2>&1 )
  echo $? >"$LAB/last-rc"
}

# The four corpora lines (the 5th field is the clone dir under $CLONES).
line_match="lab file:///dev/null $SGOOD 999 lab-head"
line_wrong="lab file:///dev/null $SPARENT 999 lab-head"
line_missing="lab file:///dev/null $SGOOD 999 lab-absent"
line_default="lab file:///dev/null $SPARENT 999"
line_ci_match="lab file:///dev/null $SGOOD 0:999 lab-head"
line_ci_over="lab file:///dev/null $SGOOD 999:0 lab-head"
line_ci_unmeasured=$line_match
line_dev_over=$line_ci_match
line_ci_invalid="lab file:///dev/null $SGOOD 999:invalid lab-head"

mk_lab "$LAB/shipped" "$GATE"

case_holds() { # NAME TRANSCRIPT RC -> 0 when the case behaved as its name says
  local name=$1 t=$2 rc=$3
  case $name in
    match)
      [[ $t == *"public revision (lab): tree $HEADCLONE at sha $SGOOD"* ]] &&
      [[ $t == *"match baseline exactly (lab)"* ]] &&
      [[ $t == *"RESULT: PASS"* ]] && (( rc == 0 )) ;;
    wrong-rev)
      [[ $t == *"refusing to measure another revision (fail-closed)"* ]] &&
      [[ $t != *"match baseline exactly (lab)"* ]] &&
      [[ $t == *"RESULT: FAIL"* ]] && (( rc == 1 )) ;;
    missing)
      [[ $t == *"declared clone $CLONES/lab-absent is missing"* ]] &&
      [[ $t == *"RESULT: FAIL"* ]] && (( rc == 1 )) ;;
    default-skip)
      [[ $t == *"SKIP public corpus (lab)"* ]] &&
      [[ $t == *"clone HEAD"* ]] &&
      [[ $t != *"RESULT: FAIL"* ]] && (( rc == 2 )) ;;
    ci-match)
      case_holds match "$t" "$rc" && [[ $t == *"public gate: column=ci"* ]] ;;
    ci-over|dev-over)
      [[ $t == *"public time ceiling (lab): "*"s > 0s"* ]] &&
      [[ $t == *"RESULT: FAIL"* ]] && (( rc == 1 )) ;;
    ci-unmeasured)
      [[ $t == *"public time ceiling (lab): no ci ceiling measured"* ]] &&
      [[ $t != *"<= 999s"* ]] && (( rc == 1 )) ;;
    ci-invalid)
      [[ $t == *"public time ceiling (lab): invalid ci ceiling invalid"* ]] &&
      [[ $t == *"RESULT: FAIL"* ]] && (( rc == 1 )) ;;
  esac
}

run_named() { # LABDIR NAME -> echoes transcript, sets RC; picks the line by name
  local labdir=$1 name=$2 line cf
  COL=dev
  case $name in
    match)        line=$line_match ;;
    wrong-rev)    line=$line_wrong ;;
    missing)      line=$line_missing ;;
    default-skip) line=$line_default ;;
    ci-match)     line=$line_ci_match; COL=ci ;;
    ci-over)      line=$line_ci_over; COL=ci ;;
    ci-unmeasured) line=$line_ci_unmeasured; COL=ci ;;
    ci-invalid)   line=$line_ci_invalid; COL=ci ;;
    dev-over)     line=$line_dev_over; COL=default ;;
  esac
  cf=$(corpora_file "$labdir" "$name" "$line")
  run_gate "$labdir" "$cf" "$labdir/art-$name"
}

say 'silence side — the shipped gate is fail-closed over a declared clone dir'
for name in match wrong-rev missing default-skip ci-match ci-over ci-unmeasured ci-invalid dev-over; do
  t=$(run_named "$LAB/shipped" "$name")
  RC=$(cat "$LAB/last-rc")
  if case_holds "$name" "$t" "$RC"; then
    ok "case $name behaved as declared (rc=$RC)"
  else
    bad "case $name misbehaved (rc=$RC): $(printf '%s' "$t" | grep -E 'lab\)|fail-closed|SKIP|RESULT' | tr '\n' '|')"
  fi
done

say 'accusation side — each guard removed, only its named behavior changes'
# name|case that must flip|literal needle|literal replacement
mutants=(
  'missing_not_failclosed|missing|-n $clone_dir && ! -d $clone|-n "" && ! -d $clone'
  'wrongrev_not_failclosed|wrong-rev|-n $clone_dir && $head != "$sha"|-n "" && $head != "$sha"'
  'ci_reads_dev_column|ci-over|ci)  limit=$ci_ceiling ;;|ci)  limit=$dev_ceiling ;;'
  'ci_borrows_dev_ceiling|ci-unmeasured|ci)  limit=$ci_ceiling ;;|ci)  limit=${ci_ceiling:-$dev_ceiling} ;;'
  'dev_defaults_to_ci|dev-over|COLUMN=${PUBLIC_COLUMN:-dev}|COLUMN=${PUBLIC_COLUMN:-ci}'
)
for entry in "${mutants[@]}"; do
  IFS='|' read -r mname mcase needle repl <<<"$entry"
  mgate=$LAB/mutant-$mname
  mkdir -p "$mgate"
  if ! mutate "$GATE" "$LAB/$mname.public-gate.sh" "$needle" "$repl"; then
    bad "INVALIDO: mutant $mname changed no bytes (the needle matched nothing)"
    continue
  fi
  if cmp -s "$GATE" "$LAB/$mname.public-gate.sh"; then
    bad "INVALIDO-cmp: mutant $mname changed no bytes"
    continue
  fi
  bash -n "$LAB/$mname.public-gate.sh" || { bad "INVALIDO-parse: mutant $mname does not parse"; continue; }
  mk_lab "$mgate" "$LAB/$mname.public-gate.sh"

  t=$(run_named "$mgate" "$mcase")
  RC=$(cat "$LAB/last-rc")
  if [[ $mcase == missing || $mcase == wrong-rev ]]; then
    reproduced=0
    [[ $t == *"SKIP public corpus (lab)"* && $t != *"RESULT: FAIL"* && $RC == 2 ]] && reproduced=1
  else
    reproduced=0
    case_holds match "$t" "$RC" && reproduced=1
  fi
  if (( ! reproduced )); then
    bad "mutant $mname did NOT reproduce the named defect in $mcase (rc=$RC)"
    continue
  fi
  # Positive control: the happy path must survive the mutation.
  tg=$(run_named "$mgate" match)
  RC=$(cat "$LAB/last-rc")
  if case_holds match "$tg" "$RC"; then
    ok "mutant $mname accused by case $mcase (match case still PASS)"
  else
    bad "mutant $mname breaks case match too — broken mutation, not a demonstration"
  fi
done

# Exercise the workflow's actual shell blocks, not a second implementation of
# its skip policy. Extract only by step name/indentation; no behavior needles.
say 'workflow controls — only the intentional GitLab SKIP may pass'
wf=$LAB/workflow
mkdir -p "$wf/scripts/public-baseline" "$wf/fakebin"
python3 - "$ROOT/.github/workflows/gauntlet.yml" "$wf" <<'PY'
import pathlib, sys
lines = pathlib.Path(sys.argv[1]).read_text().splitlines()
for name, output in [
    ("Public corpus gate (only GitLab baseline may be absent)", "verdict.sh"),
    ("Shallow-clone each declared repo at its pinned sha", "clone.sh"),
]:
    start = lines.index("      - name: " + name) + 1
    while start < len(lines) and not lines[start].startswith("      - "):
        if lines[start] == "        run: |":
            start += 1
            break
        start += 1
    body = []
    for line in lines[start:]:
        if not line.startswith("          "):
            break
        body.append(line[10:])
    if not body:
        sys.exit("missing shell block: " + name)
    pathlib.Path(sys.argv[2], output).write_text("\n".join(body) + "\n")
PY
if [[ $? != 0 ]]; then
  bad 'workflow extraction failed'
else
  cat >"$wf/scripts/public-gate.sh" <<'SH'
#!/bin/sh
printf '%s\n' "$WORKFLOW_TRANSCRIPT"
exit "$WORKFLOW_RC"
SH
  chmod +x "$wf/scripts/public-gate.sh"
  expected_skip='public corpus gate skipped (not judged on this machine): public corpus (gitlab-foss)'
  run_verdict() {
    ( cd "$wf" && WORKFLOW_RC="$2" WORKFLOW_TRANSCRIPT="$3" \
        bash -eo pipefail "$1" >"$wf/verdict.log" 2>&1 )
  }
  verdict_holds() {
    local script=$1 name=$2 rc=0
    case $name in
      complete) run_verdict "$script" 0 'RESULT: PASS' || rc=$?; (( rc == 0 )) ;;
      intentional) run_verdict "$script" 2 "$expected_skip" || rc=$?; (( rc == 0 )) ;;
      failed) run_verdict "$script" 1 "$expected_skip" || rc=$?; (( rc != 0 )) ;;
      crashed) run_verdict "$script" 7 "$expected_skip" || rc=$?; (( rc != 0 )) ;;
      other-skip)
        run_verdict "$script" 2 "${expected_skip} public corpus (rails)" || rc=$?
        (( rc != 0 )) ;;
    esac
  }
  for name in complete intentional failed crashed other-skip; do
    if verdict_holds "$wf/verdict.sh" "$name"; then ok "workflow $name";
    else bad "workflow $name"; fi
  done
  for name in failed other-skip; do
    case $name in
      failed) needle='"$rc" -eq 2'; replacement='"$rc" -ne 0' ;;
      other-skip) needle="grep -Fxq '$expected_skip'"; replacement="true #";;
    esac
    mutant=$wf/verdict-$name.sh
    if ! mutate "$wf/verdict.sh" "$mutant" "$needle" "$replacement" ||
        cmp -s "$wf/verdict.sh" "$mutant"; then
      bad "INVALIDO-cmp: workflow $name"; continue
    fi
    if ! bash -n "$mutant"; then bad "INVALIDO-parse: workflow $name"; continue; fi
    if verdict_holds "$mutant" "$name"; then
      bad "workflow mutant not accused by $name"
    elif verdict_holds "$mutant" intentional && verdict_holds "$mutant" complete; then
      ok "workflow mutant accused by $name (allowed outcomes survive)"
    else
      bad "workflow $name mutation broke an allowed outcome"
    fi
  done

  # Real clone block, fake transport: no network and no real git mutation.
  printf 'lab file:///lab %s 999 lab-head\ngitlab-foss file:///absent %s 999\n' \
    "$SGOOD" "$SGOOD" >"$wf/scripts/public-corpora.txt"
  printf '{}\n' >"$wf/scripts/public-baseline/lab.jsonl"
  cat >"$wf/fakebin/git" <<'SH'
#!/bin/sh
printf '%s\n' "$*" >>"$GIT_WITNESS"
exit 0
SH
  chmod +x "$wf/fakebin/git"
  run_clone() {
    : >"$wf/git.log"
    ( cd "$wf" && PATH="$wf/fakebin:$PATH" GIT_WITNESS="$wf/git.log" \
        PUBLIC_CORPORA_ROOT="$wf/corpora" bash -eo pipefail "$1" >"$wf/clone.log" 2>&1 )
  }
  if run_clone "$wf/clone.sh" &&
      grep -Fq "fetch -q --depth 1 origin $SGOOD" "$wf/git.log" &&
      grep -Fq 'remote add origin file:///lab' "$wf/git.log" &&
      ! grep -Fq 'file:///absent' "$wf/git.log"; then
    ok 'workflow provisions measured corpus and leaves unmeasured corpus alone'
  else
    bad 'workflow clone baseline guard'
  fi
  if ! mutate "$wf/clone.sh" "$wf/clone-mutant.sh" \
      '[ -s "scripts/public-baseline/$id.jsonl" ] || continue' ':' ||
      cmp -s "$wf/clone.sh" "$wf/clone-mutant.sh"; then
    bad 'INVALIDO-cmp: clone baseline guard'
  elif ! bash -n "$wf/clone-mutant.sh"; then
    bad 'INVALIDO-parse: clone baseline guard'
  elif run_clone "$wf/clone-mutant.sh" &&
      grep -Fq 'remote add origin file:///absent' "$wf/git.log" &&
      grep -Fq 'remote add origin file:///lab' "$wf/git.log"; then
    ok 'clone mutant fetches unmeasured corpus (measured control survives)'
  else
    bad 'clone mutant did NOT reproduce the unmeasured fetch'
  fi
fi

say 'summary'
if (( failed )); then echo "RESULT: FAIL (lab kept at $LAB)"; exit 1; fi
rm -rf "$LAB"
echo 'RESULT: PASS (both sides proved)'
