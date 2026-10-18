#!/usr/bin/env bash
# Two-sided proof for the fail-closed public-corpus workflow.
#
# The public gate takes an optional 5th declaration field, <clone-dir>, that
# names the exact tree to measure for a re-pin. When it is present the gate is
# fail-closed: the path must exist AND its HEAD must be the declared sha, else
# the repo FAILS — never a SKIP, never a silent measurement of the wrong
# revision. Without the field the historical semantics stand (a missing or
# wrong-HEAD canonical clone is a SKIP).
#
# Silence side: the shipped gate still proves its clone-path and ceiling
# controls. The workflow controls also exercise the one shared
# scripts/public-corpora-fetch.sh implementation with a local fixture remote:
#   missing, already-at-pin, wrong-HEAD, no-baseline, failed-fetch, and the
#   PUBLIC_GATE_CLONE=1 gate path.
#
# Accusation side: every guard has a labelled mutant and its named case must
# accuse it. Every mutation is diff-guarded (a replacement that changed
# nothing is INVALIDO), and the happy path remains a positive control.
#
# Timing controls also select measured dev/CI columns independently of CI,
# refuse unmeasured/invalid CI ceilings, and mutate column selection.
# Workflow verdict controls accept only the intentional GitLab SKIP.
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
# its skip policy. Resolve its effective shell as GitHub does, not the shell
# running this selftest. Extract by key names/indentation; no PyYAML dependency.
say 'workflow controls — only the intentional GitLab SKIP may pass'
wf=$LAB/workflow
mkdir -p "$wf/scripts/public-baseline" "$wf/fakebin"
extract_workflow() {
  python3 - "$@" <<'PY'
import json, pathlib, re, sys
lines = pathlib.Path(sys.argv[1]).read_text().splitlines()

def scalar(value):
    value = value.strip()
    if value.startswith("'"):
        match = re.fullmatch(r"'((?:[^']|'')*)'\s*(?:#.*)?", value)
        if match:
            return match[1].replace("''", "'")
    elif value.startswith('"'):
        match = re.fullmatch(r'("(?:[^"\\]|\\.)*")\s*(?:#.*)?', value)
        if match:
            return json.loads(match[1])
    else:
        return re.split(r"\s+#", value, maxsplit=1)[0].strip()
    sys.exit("workflow extraction: unsupported quoted scalar: " + value)

def end_of_block(start, limit, indent):
    for at in range(start + 1, limit):
        if lines[at].strip():
            if len(lines[at]) - len(lines[at].lstrip()) <= indent:
                return at
    return limit

def field(scope, key):
    start, end, indent = scope
    for at in range(start + 1, end):
        match = re.fullmatch(" " * indent + re.escape(key) + r":(?:\s+(.*))?", lines[at])
        if match:
            return at, scalar(match[1] or "")
    return None

def block(scope, key):
    found = field(scope, key)
    if found is None:
        return None
    at, value = found
    if value and not value.startswith("#"):
        sys.exit("workflow extraction: expected indented block: " + key)
    return at, end_of_block(at, scope[1], scope[2]), scope[2] + 2

def default_shell(scope):
    for key in ("defaults", "run"):
        scope = block(scope, key)
        if scope is None:
            return None
    return field(scope, "shell")

root = (-1, len(lines), 0)
jobs = block(root, "jobs")
public = block(jobs, "public") if jobs else None
steps = block(public, "steps") if public else None
if steps is None:
    sys.exit("workflow extraction: missing public job steps")

def step(name):
    start, end, indent = steps
    for at in range(start + 1, end):
        match = re.fullmatch(" " * indent + r"- name:\s+(.*)", lines[at])
        if match and scalar(match[1]) == name:
            return at, end_of_block(at, end, indent), indent + 2
    sys.exit("public job has no step: " + name)

fetch = field(step("Restore pinned public corpora"), "run")
if fetch is None:
    sys.exit("public job does not invoke scripts/public-corpora-fetch.sh")
name = "Public corpus gate (only GitLab baseline may be absent)"
verdict = step(name)
run = field(verdict, "run")
if run is None or run[1] != "|":
    sys.exit("missing shell block: " + name)
body_end = end_of_block(run[0], verdict[1], verdict[2])
body = [line[verdict[2] + 2:] for line in lines[run[0] + 1:body_end]]
if not body:
    sys.exit("empty shell block: " + name)

# Most specific wins, including an unsupported override (fail closed).
selected = field(verdict, "shell")
if selected is None:
    selected = default_shell(public)
if selected is None:
    selected = default_shell(root)
shell = selected[1] if selected else None
templates = {None: "bash -e {0}", "bash": "bash --noprofile --norc -eo pipefail {0}",
             "sh": "sh -e {0}"}
if shell in templates:
    template = templates[shell]
elif "{0}" in shell:
    template = shell
else:
    sys.exit("workflow extraction: unsupported shell: " + repr(shell))
destination = pathlib.Path(sys.argv[2])
destination.mkdir(parents=True, exist_ok=True)
(destination / "verdict.sh").write_text("\n".join(body) + "\n")
(destination / "verdict-shell.txt").write_text(template + "\n")
(destination / "clone.sh").write_text(fetch[1] + "\n")
PY
}
extract_workflow "$ROOT/.github/workflows/gauntlet.yml" "$wf"
if [[ $? != 0 ]]; then
  bad 'workflow extraction failed'
else
  ok "workflow effective shell: $(cat "$wf/verdict-shell.txt")"
  shell_mutant="$LAB/gauntlet-no-workflow-shell.yml"
  shell_mutant_wf="$LAB/workflow-no-workflow-shell"
  shell_needle=$'defaults:\n  run:\n    shell: bash\n'
  if ! mutate "$ROOT/.github/workflows/gauntlet.yml" "$shell_mutant" "$shell_needle" '' ||
      cmp -s "$ROOT/.github/workflows/gauntlet.yml" "$shell_mutant"; then
    bad 'INVALIDO-cmp: workflow shell mutant'
  elif ! extract_workflow "$shell_mutant" "$shell_mutant_wf"; then
    bad 'workflow shell mutant extraction failed'
  elif grep -Fxq 'bash --noprofile --norc -eo pipefail {0}' \
      "$shell_mutant_wf/verdict-shell.txt"; then
    bad 'INVALIDO: workflow shell mutant still resolves to pipefail'
  else
    ok 'workflow shell mutant resolves without pipefail'
  fi
  if grep -Fq './scripts/public-corpora-fetch.sh' "$wf/clone.sh"; then
    ok 'workflow public step invokes the shared corpus fetcher'
  else
    bad 'workflow public step does not invoke the shared corpus fetcher'
  fi
  cp "$ROOT/scripts/public-corpora-fetch.sh" "$wf/scripts/public-corpora-fetch.sh"
  chmod +x "$wf/scripts/public-corpora-fetch.sh"
  cat >"$wf/scripts/public-gate.sh" <<'SH'
#!/bin/sh
printf '%s\n' "$WORKFLOW_TRANSCRIPT"
exit "$WORKFLOW_RC"
SH
  chmod +x "$wf/scripts/public-gate.sh"
  expected_skip='public corpus gate skipped (not judged on this machine): public corpus (gitlab-foss)'
  run_verdict() {
    ( cd "$wf" && WORKFLOW_RC="$2" WORKFLOW_TRANSCRIPT="$3" \
        python3 - "$1" "${4:-$wf/verdict-shell.txt}" >"$wf/verdict.log" 2>&1 <<'PY'
import os, pathlib, shlex, sys
template = pathlib.Path(sys.argv[2]).read_text().strip()
command = [arg.replace("{0}", sys.argv[1]) for arg in shlex.split(template)]
os.execvp(command[0], command)
PY
    )
  }
  verdict_holds() {
    local script=$1 name=$2 shell=${3:-$wf/verdict-shell.txt} rc=0
    case $name in
      complete) run_verdict "$script" 0 'RESULT: PASS' "$shell" || rc=$?; (( rc == 0 )) ;;
      intentional) run_verdict "$script" 2 "$expected_skip" "$shell" || rc=$?; (( rc == 0 )) ;;
      failed) run_verdict "$script" 1 "$expected_skip" "$shell" || rc=$?; (( rc != 0 )) ;;
      crashed) run_verdict "$script" 7 "$expected_skip" "$shell" || rc=$?; (( rc != 0 )) ;;
      other-skip)
        run_verdict "$script" 2 "${expected_skip} public corpus (rails)" "$shell" || rc=$?
        (( rc != 0 )) ;;
    esac
  }
  for name in complete intentional failed crashed other-skip; do
    if verdict_holds "$wf/verdict.sh" "$name"; then ok "workflow $name";
    else bad "workflow $name"; fi
  done
  if [[ -f "$shell_mutant_wf/verdict.sh" ]]; then
    for name in failed crashed; do
      if verdict_holds "$shell_mutant_wf/verdict.sh" "$name" \
          "$shell_mutant_wf/verdict-shell.txt"; then
        bad "workflow shell mutant not accused by $name"
      elif verdict_holds "$shell_mutant_wf/verdict.sh" intentional \
          "$shell_mutant_wf/verdict-shell.txt" &&
          verdict_holds "$shell_mutant_wf/verdict.sh" complete \
          "$shell_mutant_wf/verdict-shell.txt"; then
        ok "workflow shell mutant accused by $name (allowed outcomes survive)"
      else
        bad "workflow shell mutant broke an allowed outcome for $name"
      fi
    done
  fi
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

  # The fixture remote is local, but this is still a real depth-1 fetch. Git's
  # upload-pack setting makes fetch-by-object-id work across supported Git
  # versions, while the fetcher's fallback covers servers that decline it.
  REMOTE=$LAB/public-remote.git
  git_clean clone -q --bare "$HEADCLONE" "$REMOTE"
  git_clean -C "$REMOTE" config uploadpack.allowAnySHA1InWant true
  fetch_case() {
    local root=$1 declarations=$2 baseline=$3 out=$4 rc
    ( cd "$wf" && PUBLIC_CORPORA_ROOT="$root" \
        PUBLIC_CORPORA_FILE="$declarations" PUBLIC_BASELINE_DIR="$baseline" \
        bash "$wf/scripts/public-corpora-fetch.sh" >"$out" 2>&1 )
    rc=$?
    printf '%s\n' "$rc" >"$out.rc"
    return "$rc"
  }
  baseline="$wf/scripts/public-baseline"
  printf '{}\n' >"$baseline/lab.jsonl"

  # missing -> fetched atomically at the final path
  missing_root=$wf/fetch-missing
  printf 'lab %s %s 999\n' "$REMOTE" "$SGOOD" >"$wf/missing.txt"
  if fetch_case "$missing_root" "$wf/missing.txt" "$baseline" "$wf/missing.log" &&
      grep -Fq "fetched lab @ ${SGOOD:0:7}" "$wf/missing.log" &&
      [[ $(git_clean -C "$missing_root/lab" rev-parse HEAD) == "$SGOOD" ]]; then
    ok 'fetcher missing corpus'
  else
    bad 'fetcher missing corpus'
  fi

  # already at pin -> no network (the URL is intentionally nonexistent)
  already_root=$wf/fetch-already
  mkdir -p "$already_root"
  git_clean clone -q --no-local "$REMOTE" "$already_root/lab"
  git_clean -C "$already_root/lab" checkout -q --detach "$SGOOD"
  printf 'lab %s %s 999\n' "$LAB/does-not-exist" "$SGOOD" >"$wf/already.txt"
  if fetch_case "$already_root" "$wf/already.txt" "$baseline" "$wf/already.log" &&
      grep -Fq "ok lab already at ${SGOOD:0:7}" "$wf/already.log"; then
    ok 'fetcher already-at-pin avoids network'
  else
    bad 'fetcher already-at-pin made a network call'
  fi

  # other sha -> FAIL without touching the tree
  other_root=$wf/fetch-other
  mkdir -p "$other_root"
  git_clean clone -q --no-local "$REMOTE" "$other_root/lab"
  git_clean -C "$other_root/lab" checkout -q --detach "$SPARENT"
  before=$(git_clean -C "$other_root/lab" rev-parse HEAD)
  printf 'lab %s %s 999\n' "$REMOTE" "$SGOOD" >"$wf/other.txt"
  if ! fetch_case "$other_root" "$wf/other.txt" "$baseline" "$wf/other.log" &&
      [[ $(git_clean -C "$other_root/lab" rev-parse HEAD) == "$before" ]] &&
      grep -Fq 'refusing to move a tree' "$wf/other.log"; then
    ok 'fetcher refuses a differently pinned tree'
  else
    bad 'fetcher moved or accepted a differently pinned tree'
  fi

  # a plain dir nested INSIDE another checkout that sits at the pinned sha is
  # still "not a git repo": the host's HEAD must never be borrowed.
  nested_host=$wf/fetch-nested-host
  git_clean clone -q --no-local "$REMOTE" "$nested_host"
  git_clean -C "$nested_host" checkout -q --detach "$SGOOD"
  mkdir -p "$nested_host/corpora/lab"
  if ! fetch_case "$nested_host/corpora" "$wf/missing.txt" "$baseline" "$wf/nested.log" &&
      grep -Fq 'is not a git repo' "$wf/nested.log"; then
    ok 'fetcher refuses a plain dir nested in another checkout'
  else
    bad 'fetcher borrowed a host checkout HEAD for a plain dir'
  fi

  # no baseline -> SKIP and never fetch, even with a valid local remote
  nobase_root=$wf/fetch-no-baseline
  printf 'nobase %s %s 999\n' "$REMOTE" "$SGOOD" >"$wf/nobase.txt"
  if fetch_case "$nobase_root" "$wf/nobase.txt" "$wf/no-baseline" "$wf/nobase.log" &&
      grep -Fq 'skip nobase (no baseline; never fetched)' "$wf/nobase.log" &&
      [[ ! -e "$nobase_root/nobase" ]]; then
    ok 'fetcher leaves an unbaselined corpus alone'
  else
    bad 'fetcher fetched an unbaselined corpus'
  fi

  # failing fetch -> no final tree and no partial directory. TWO failing repos,
  # because the exit trap alone cleans the LAST partial: only the per-failure
  # cleanup keeps the first one from leaking, and only two repos can tell.
  bad_root=$wf/fetch-bad
  badsha=ffffffffffffffffffffffffffffffffffffffff
  printf 'bad %s %s 999\nbad2 %s %s 999\n' "$REMOTE" "$badsha" "$REMOTE" "$badsha" >"$wf/bad.txt"
  # A baseline makes the fetcher actually try (and fail); without one it
  # would skip both and this case would prove nothing about cleanup.
  printf '{}\n' >"$baseline/bad.jsonl"
  printf '{}\n' >"$baseline/bad2.jsonl"
  no_partials() { # ROOT
    [[ ! -e "$1/bad" && ! -e "$1/bad2" ]] && ! compgen -G "$1/bad*.partial.*" >/dev/null
  }
  if ! fetch_case "$bad_root" "$wf/bad.txt" "$baseline" "$wf/bad.log" &&
      no_partials "$bad_root"; then
    ok 'fetcher removes a failed partial fetch'
  else
    bad 'fetcher left a failed partial fetch behind'
  fi

  # PUBLIC_GATE_CLONE=1 uses the same fetcher and yields a shallow clone.
  gate=$wf/gate-path
  mkdir -p "$gate/scripts" "$gate/target/release" "$gate/scripts/public-baseline"
  cp "$GATE" "$gate/scripts/public-gate.sh"
  cp "$ROOT/scripts/public-corpora-fetch.sh" "$gate/scripts/public-corpora-fetch.sh"
  chmod +x "$gate/scripts/public-gate.sh" "$gate/scripts/public-corpora-fetch.sh"
  printf 'gate %s %s 999\n' "$REMOTE" "$SGOOD" >"$gate/scripts/public-corpora.txt"
  printf '{"code":"E0101","column":5,"line":3,"message":"lab","path":"gate/f.rb","severity":"error"}\n' \
    >"$gate/scripts/public-baseline/gate.jsonl"
  cat >"$gate/target/release/ita" <<'SH'
#!/bin/sh
printf '{"code":"E0101","column":5,"line":3,"message":"lab","path":"%s/f.rb","severity":"error"}\n' "$2"
SH
  chmod +x "$gate/target/release/ita"
  if ( cd "$gate" && PUBLIC_GATE_CLONE=1 \
      PUBLIC_CORPORA_FILE="$gate/scripts/public-corpora.txt" \
      PUBLIC_BASELINE_DIR="$gate/scripts/public-baseline" \
      PUBLIC_CORPORA_ROOT="$gate/corpora" ART="$gate/art" \
      bash "$gate/scripts/public-gate.sh" >"$gate/gate.log" 2>&1 ) &&
      git_clean -C "$gate/corpora/gate" rev-parse --is-shallow-repository | grep -Fxq true &&
      grep -Fq 'RESULT: PASS' "$gate/gate.log"; then
    ok 'gate auto-restore is a shallow pinned fetch'
  else
    bad 'gate auto-restore did not produce a shallow pinned fetch'
  fi

  # Accuse the baseline guard: its mutant fetches the unbaselined fixture.
  if ! mutate "$wf/scripts/public-corpora-fetch.sh" "$wf/fetch-mutant-baseline.sh" \
      'if [[ ! -s $base ]]; then' 'if [[ -s $base ]]; then' ||
      cmp -s "$wf/scripts/public-corpora-fetch.sh" "$wf/fetch-mutant-baseline.sh"; then
    bad 'INVALIDO-cmp: fetcher baseline guard'
  elif ! bash -n "$wf/fetch-mutant-baseline.sh"; then
    bad 'INVALIDO-parse: fetcher baseline guard'
  else
    mutant_root=$wf/fetch-mutant-baseline
    if ( cd "$wf" && PUBLIC_CORPORA_ROOT="$mutant_root" \
        PUBLIC_CORPORA_FILE="$wf/nobase.txt" PUBLIC_BASELINE_DIR="$wf/no-baseline" \
        bash "$wf/fetch-mutant-baseline.sh" >"$wf/fetch-mutant-baseline.log" 2>&1 ) &&
        [[ -d "$mutant_root/nobase" ]]; then
      ok 'fetcher baseline mutant accused by no-baseline'
    else
      bad 'fetcher baseline mutant did not fetch the unbaselined corpus'
    fi
  fi

  # Accuse the already-at-pin guard: its mutant rejects a pinned existing tree.
  if ! mutate "$wf/scripts/public-corpora-fetch.sh" "$wf/fetch-mutant-pin.sh" \
      'if [[ $head == "$sha" ]]; then' 'if [[ $head != "$sha" ]]; then' ||
      cmp -s "$wf/scripts/public-corpora-fetch.sh" "$wf/fetch-mutant-pin.sh"; then
    bad 'INVALIDO-cmp: fetcher already-at-pin guard'
  elif ! bash -n "$wf/fetch-mutant-pin.sh"; then
    bad 'INVALIDO-parse: fetcher already-at-pin guard'
  elif ( cd "$wf" && PUBLIC_CORPORA_ROOT="$already_root" \
      PUBLIC_CORPORA_FILE="$wf/already.txt" PUBLIC_BASELINE_DIR="$baseline" \
      bash "$wf/fetch-mutant-pin.sh" >"$wf/fetch-mutant-pin.log" 2>&1 ); then
    bad 'fetcher already-at-pin mutant was not accused'
  else
    ok 'fetcher already-at-pin mutant accused by already-at-pin'
  fi

  # Accuse the per-failure cleanup: without it the FIRST failed partial leaks
  # (the exit trap only ever holds the last one).
  if ! mutate "$wf/scripts/public-corpora-fetch.sh" "$wf/fetch-mutant-cleanup.sh" \
      $'\n    cleanup_partial\n' $'\n' ||
      cmp -s "$wf/scripts/public-corpora-fetch.sh" "$wf/fetch-mutant-cleanup.sh"; then
    bad 'INVALIDO-cmp: fetcher partial cleanup'
  elif ! bash -n "$wf/fetch-mutant-cleanup.sh"; then
    bad 'INVALIDO-parse: fetcher partial cleanup'
  else
    mutant_root=$wf/fetch-mutant-cleanup
    ( cd "$wf" && PUBLIC_CORPORA_ROOT="$mutant_root" \
        PUBLIC_CORPORA_FILE="$wf/bad.txt" PUBLIC_BASELINE_DIR="$baseline" \
        bash "$wf/fetch-mutant-cleanup.sh" >"$wf/fetch-mutant-cleanup.log" 2>&1 )
    if no_partials "$mutant_root"; then
      bad 'fetcher partial-cleanup mutant was not accused'
    else
      ok 'fetcher partial-cleanup mutant accused by failed-fetch'
    fi
  fi

  # Accuse the top-level check: without it the nested plain dir reads the
  # host checkout's HEAD and passes as "already at" the pin.
  if ! mutate "$wf/scripts/public-corpora-fetch.sh" "$wf/fetch-mutant-nested.sh" \
      ' || [[ -n $prefix ]]; then' '; then' ||
      cmp -s "$wf/scripts/public-corpora-fetch.sh" "$wf/fetch-mutant-nested.sh"; then
    bad 'INVALIDO-cmp: fetcher top-level check'
  elif ! bash -n "$wf/fetch-mutant-nested.sh"; then
    bad 'INVALIDO-parse: fetcher top-level check'
  elif ( cd "$wf" && PUBLIC_CORPORA_ROOT="$nested_host/corpora" \
      PUBLIC_CORPORA_FILE="$wf/missing.txt" PUBLIC_BASELINE_DIR="$baseline" \
      bash "$wf/fetch-mutant-nested.sh" >"$wf/fetch-mutant-nested.log" 2>&1 ) &&
      grep -Fq 'ok lab already at' "$wf/fetch-mutant-nested.log"; then
    ok 'fetcher top-level mutant accused by nested-host'
  else
    bad 'fetcher top-level mutant was not accused'
  fi
fi

say 'summary'
if (( failed )); then echo "RESULT: FAIL (lab kept at $LAB)"; exit 1; fi
rm -rf "$LAB"
echo 'RESULT: PASS (both sides proved)'
