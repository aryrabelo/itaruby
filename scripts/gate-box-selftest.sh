#!/usr/bin/env bash
# gate-box-selftest.sh — the two-sided proof that scripts/dev mirrors the
# exact source tree, serializes the persistent target, copies the corpus map,
# and scrubs caller-owned gate paths. Every omission below is a portable
# mutant of the shipped script and must be accused by its named case.
#
# The lab is disposable and never touches the real checkout. A fixture git
# repository receives a copy of scripts/dev and a tiny script that reports the
# HEAD, write-tree, target-dir environment, and map hash it actually saw.
set -uo pipefail

cd "$(dirname "$0")/.."
ROOT=$PWD
# A selftest launched from inside a gate-box run (gate c2b) must not inherit
# that run's lock state: the fixture's own dev would skip its lock and read
# the OUTER source. scripts/dev scrubs these itself (case f proves it); this
# is the selftest refusing to depend on that for its own safety.
unset ITA_GATE_BOX ITA_GATE_BOX_LOCKED ITA_GATE_BOX_SOURCE \
  ITA_GATE_BOX_TARGET_SHA ITA_GATE_BOX_COMMON ITA_GATE_BOX_WAIT
LAB=${GATE_BOX_LAB:-$HOME/Sites/temp-files/gate-box-selftest-$(date +%Y%m%d-%H%M%S)}
failed=0
bg_pids=()

say() { printf '\n=== %s\n' "$1"; }
ok() { printf 'PASS %s\n' "$1"; }
bad() { printf 'FAIL %s\n' "$1"; failed=1; }

if [[ $LAB != /* ]]; then
  printf 'FAIL GATE_BOX_LAB must be an ABSOLUTE path, got %q\n' "$LAB" >&2
  exit 1
fi
case $LAB in
  / | "$ROOT" | "$ROOT"/)
    printf 'FAIL GATE_BOX_LAB refuses to use %q as a scratch lab\n' "$LAB" >&2
    exit 1
    ;;
esac

cleanup() {
  local pid
  for pid in "${bg_pids[@]}"; do
    kill "$pid" 2>/dev/null || :
  done
  for pid in "${bg_pids[@]}"; do
    wait "$pid" 2>/dev/null || :
  done
}
trap cleanup EXIT INT TERM
rm -rf "$LAB"
mkdir -p "$LAB" || { echo "FAIL could not create lab $LAB" >&2; exit 1; }
printf 'lab: %s (recreated fresh for this run)\n' "$LAB"

git_clean() {
  env -u GIT_DIR -u GIT_INDEX_FILE -u GIT_WORK_TREE -u GIT_PREFIX \
    -u GIT_COMMON_DIR -u GIT_AUTHOR_DATE -u GIT_COMMITTER_DATE git "$@"
}

fixture=$LAB/fixture
mkdir -p "$fixture/scripts"
cp "$ROOT/scripts/dev" "$fixture/scripts/dev"
chmod +x "$fixture/scripts/dev"
cat >"$fixture/scripts/stub.sh" <<'STUB'
#!/usr/bin/env bash
set -uo pipefail
idx=${STUB_INDEX:?STUB_INDEX is required}
trap 'rm -f "$idx"' EXIT
head=$(env -u GIT_DIR -u GIT_INDEX_FILE -u GIT_WORK_TREE -u GIT_PREFIX \
  -u GIT_COMMON_DIR git -C "$PWD" rev-parse HEAD)
env -u GIT_DIR -u GIT_INDEX_FILE -u GIT_WORK_TREE -u GIT_PREFIX \
  -u GIT_COMMON_DIR GIT_INDEX_FILE="$idx" git -C "$PWD" read-tree HEAD
env -u GIT_DIR -u GIT_INDEX_FILE -u GIT_WORK_TREE -u GIT_PREFIX \
  -u GIT_COMMON_DIR GIT_INDEX_FILE="$idx" git -C "$PWD" add -A
tree=$(env -u GIT_DIR -u GIT_INDEX_FILE -u GIT_WORK_TREE -u GIT_PREFIX \
  -u GIT_COMMON_DIR GIT_INDEX_FILE="$idx" git -C "$PWD" write-tree)
if [[ -f scripts/corpora-local.txt ]]; then
  map=$(shasum -a 256 scripts/corpora-local.txt | awk '{print $1}')
else
  map=none
fi
printf 'HEAD=%s\nTREE=%s\nCTD=%s\nMAP=%s\nLOCKED=%s\n' "$head" "$tree" \
  "${CARGO_TARGET_DIR-unset}" "$map" "${ITA_GATE_BOX_LOCKED-unset}"
if [[ ${STUB_SLEEP:-0} != 0 ]]; then sleep "$STUB_SLEEP"; fi
exit "${STUB_EXIT:-0}"
STUB
chmod +x "$fixture/scripts/stub.sh"
printf 'fixture payload\n' >"$fixture/payload.rb"
printf 'scripts/corpora-local.txt\n' >"$fixture/.gitignore"
printf 'corpus-c /fixture/corpus\n' >"$fixture/scripts/corpora-local.txt"
git_clean init -q "$fixture"
git_clean -C "$fixture" config user.email gate-box-selftest@invalid
git_clean -C "$fixture" config user.name gate-box-selftest
# The first commit is the --at target; the second is the source HEAD.
git_clean -C "$fixture" add -A
git_clean -C "$fixture" commit -qm 'fixture base'
old_sha=$(git_clean -C "$fixture" rev-parse HEAD)
printf 'fixture newer payload\n' >"$fixture/payload.rb"
git_clean -C "$fixture" add payload.rb
git_clean -C "$fixture" commit -qm 'fixture newer'
new_sha=$(git_clean -C "$fixture" rev-parse HEAD)

write_tree() {
  local repo=$1 rev=${2:-} idx=$LAB/oracle-index-$$-$RANDOM tree
  if [[ -n $rev ]]; then
    env -u GIT_DIR -u GIT_INDEX_FILE -u GIT_WORK_TREE -u GIT_PREFIX \
      -u GIT_COMMON_DIR GIT_INDEX_FILE="$idx" git -C "$repo" read-tree "$rev" || return 1
  else
    env -u GIT_DIR -u GIT_INDEX_FILE -u GIT_WORK_TREE -u GIT_PREFIX \
      -u GIT_COMMON_DIR GIT_INDEX_FILE="$idx" git -C "$repo" read-tree HEAD || return 1
    env -u GIT_DIR -u GIT_INDEX_FILE -u GIT_WORK_TREE -u GIT_PREFIX \
      -u GIT_COMMON_DIR GIT_INDEX_FILE="$idx" git -C "$repo" add -A || return 1
  fi
  tree=$(env -u GIT_DIR -u GIT_INDEX_FILE -u GIT_WORK_TREE -u GIT_PREFIX \
    -u GIT_COMMON_DIR GIT_INDEX_FILE="$idx" git -C "$repo" write-tree) || return 1
  rm -f "$idx"
  printf '%s\n' "$tree"
}
map_hash() {
  if [[ -f $1/scripts/corpora-local.txt ]]; then
    shasum -a 256 "$1/scripts/corpora-local.txt" | awk '{print $1}'
  else
    printf 'none\n'
  fi
}
stub_value() { sed -n "s/^$1=//p" "$2" | tail -1; }

RUN_EXIT=0
RUN_SLEEP=0
run_repo() {
  local repo=$1 box=$2 entry=$3 out=$4 idx=$LAB/stub-index-$$-$RANDOM
  shift 4
  STUB_INDEX=$idx STUB_EXIT=$RUN_EXIT STUB_SLEEP=$RUN_SLEEP \
    CARGO_TARGET_DIR="$LAB/caller-target" ITA_GATE_BOX="$box" \
    bash "$repo/$entry" gates --run scripts/stub.sh "$@" >"$out" 2>&1
}
run_repo_bg() {
  local repo=$1 box=$2 entry=$3 out=$4 idx=$LAB/stub-index-$$-$RANDOM
  shift 4
  (
    STUB_INDEX=$idx STUB_EXIT=$RUN_EXIT STUB_SLEEP=$RUN_SLEEP \
      CARGO_TARGET_DIR="$LAB/caller-target" ITA_GATE_BOX="$box" \
      bash "$repo/$entry" gates --run scripts/stub.sh "$@" >"$out" 2>&1
  ) &
  printf '%s\n' "$!"
}

expect_source_result() {
  local label=$1 repo=$2 box=$3 entry=$4 out=$5 rev=${6:-}
  local expected_head expected_tree expected_map actual_head actual_tree actual_map actual_ctd actual_locked rc
  if [[ -n $rev ]]; then
    expected_head=$rev
    expected_tree=$(write_tree "$repo" "$rev")
  else
    expected_head=$(git_clean -C "$repo" rev-parse HEAD)
    expected_tree=$(write_tree "$repo")
  fi
  expected_map=$(map_hash "$repo")
  if [[ -n $rev ]]; then
    run_repo "$repo" "$box" "$entry" "$out" --at "$rev"
  else
    run_repo "$repo" "$box" "$entry" "$out"
  fi
  rc=$?
  actual_head=$(stub_value HEAD "$out")
  actual_tree=$(stub_value TREE "$out")
  actual_map=$(stub_value MAP "$out")
  actual_ctd=$(stub_value CTD "$out")
  actual_locked=$(stub_value LOCKED "$out")
  if (( rc == 0 )) && [[ $actual_head == "$expected_head" &&
      $actual_tree == "$expected_tree" && $actual_map == "$expected_map" &&
      $actual_ctd == unset && $actual_locked == unset ]]; then
    ok "$label"
  else
    bad "$label (expected HEAD=$expected_head TREE=$expected_tree MAP=$expected_map CTD=unset LOCKED=unset; see $out)"
  fi
}
reset_fixture() {
  git_clean -C "$1" reset --hard -q "$new_sha"
  git_clean -C "$1" clean -fd
}

say 'silence side — source and box produce the same HEAD, tree, map, and clean environment'
box=$LAB/box
out=$LAB/clean.txt
expect_source_result 'clean tree mirrors exactly' "$fixture" "$box" scripts/dev "$out"
printf 'modified tracked payload\n' >"$fixture/payload.rb"
expect_source_result 'modified tracked file overlays exactly' "$fixture" "$box" scripts/dev "$LAB/tracked.txt"
reset_fixture "$fixture"
printf 'staged payload\n' >"$fixture/payload.rb"
git_clean -C "$fixture" add payload.rb
expect_source_result 'staged change overlays exactly' "$fixture" "$box" scripts/dev "$LAB/staged.txt"
reset_fixture "$fixture"
rm "$fixture/payload.rb"
expect_source_result 'deleted tracked file overlays exactly' "$fixture" "$box" scripts/dev "$LAB/deleted.txt"
reset_fixture "$fixture"
printf 'new untracked file\n' >"$fixture/new.rb"
expect_source_result 'new untracked file mirrors exactly' "$fixture" "$box" scripts/dev "$LAB/untracked.txt"
reset_fixture "$fixture"
expect_source_result '--at older commit checks out exactly' "$fixture" "$box" scripts/dev "$LAB/at.txt" "$old_sha"
RUN_EXIT=2
run_repo "$fixture" "$box" scripts/dev "$LAB/exit2.txt"
if (( $? == 2 )); then ok 'stub exit 2 propagates through dev'; else bad 'stub exit 2 did not propagate'; fi
RUN_EXIT=0

mutate() {
  local source=$1 out=$2 needle=$3 replacement=$4
  python3 - "$source" "$out" "$needle" "$replacement" <<'PY'
import pathlib
import sys
source, out, needle, replacement = sys.argv[1:]
data = pathlib.Path(source).read_text()
if needle not in data:
    print("INVALIDO-cmp: mutation needle did not match", file=sys.stderr)
    raise SystemExit(3)
mutant = data.replace(needle, replacement, 1)
pathlib.Path(out).write_text(mutant)
PY
  if [[ $? -ne 0 ]]; then return 1; fi
  if cmp -s "$source" "$out"; then
    echo "INVALIDO-cmp: mutant is byte-identical" >&2
    return 1
  fi
  chmod +x "$out"
}
clone_fixture() {
  local dest=$1
  cp -R "$fixture" "$dest"
  chmod +x "$dest/scripts/dev" "$dest/scripts/stub.sh"
}

overlay_block=$(cat <<'EOF'
    if ! git_clean -C "$source" diff --quiet --binary HEAD; then
      git_clean -C "$source" diff --binary HEAD |
        git_clean -C "$box" apply || return 65
    fi
EOF
)
# Only the comparison goes: the assignments around it feed the header line,
# and removing them would crash the mutant (set -u) instead of letting it run
# the wrong tree, which is the defect this case must show.
verify_block=$(cat <<'EOF'
    if [[ $source_tree != "$box_tree" || $source_head != "$box_head" ]]; then
      echo "gate box does not mirror $source: tree $source_tree != $box_tree" >&2
      return 65
    fi
EOF
)

say 'accusation side — every omitted decision is caught by its named witness'
# (a) Missing tracked overlay is caught by the exact mirror guard.
a=$LAB/case-overlay
mkdir -p "$a"
clone_fixture "$a/repo"
printf 'overlay mutant payload\n' >"$a/repo/payload.rb"
mutate "$a/repo/scripts/dev" "$a/repo/scripts/dev-mutant.sh" "$overlay_block" '    : # tracked overlay removed' || bad 'tracked overlay mutant was not created'
if [[ -x $a/repo/scripts/dev-mutant.sh ]]; then
  run_repo "$a/repo" "$a/box" scripts/dev-mutant.sh "$a/out.txt"
  if (( $? == 65 )) && grep -q 'gate box does not mirror' "$a/out.txt"; then
    ok 'tracked overlay removed — mirror guard accuses'
  else
    bad 'tracked overlay removed — mirror guard did not accuse'
  fi
fi

# (b) Removing both overlay and mirror guard must still be visible to our own
# oracle: the stub ran a different write-tree.
b=$LAB/case-unverified
mkdir -p "$b"
clone_fixture "$b/repo"
printf 'unverified mutant payload\n' >"$b/repo/payload.rb"
without_overlay=$b/repo/scripts/dev-without-overlay.sh
mutate "$b/repo/scripts/dev" "$without_overlay" "$overlay_block" '    : # tracked overlay removed' || bad 'unverified overlay mutant was not created'
without_guard=$b/repo/scripts/dev-mutant.sh
mutate "$without_overlay" "$without_guard" "$verify_block" '    : # mirror guard removed' || bad 'mirror guard mutant was not created'
if [[ -x $without_guard ]]; then
  expected=$(write_tree "$b/repo")
  run_repo "$b/repo" "$b/box" scripts/dev-mutant.sh "$b/out.txt"
  run_rc=$?
  actual=$(stub_value TREE "$b/out.txt")
  if (( run_rc == 0 )) && [[ $actual != "$expected" ]] &&
      ! grep -q 'gate box does not mirror' "$b/out.txt"; then
    ok 'overlay and verify guard removed — oracle says box ran another tree'
  else
    bad 'overlay and verify guard removed — oracle did not accuse'
  fi
fi

# (c) Removing lock acquisition allows overlap; the shipped script must refuse
# the same second invocation with the named exit 75.
lock_block='  acquire_gate_box_lock "${helper_args[@]}"'
for kind in shipped mutant; do
  c=$LAB/case-lock-$kind
  mkdir -p "$c"
  clone_fixture "$c/repo"
  entry=scripts/dev
  if [[ $kind == mutant ]]; then
    mutate "$c/repo/scripts/dev" "$c/repo/scripts/dev-mutant.sh" "$lock_block" \
      '  run_gate_box "$ROOT" "$box" "$at_rev" "$run_script" "${forwarded[@]}"' ||
      bad 'lock mutant was not created'
    entry=scripts/dev-mutant.sh
  fi
  RUN_SLEEP=2
  first=$(run_repo_bg "$c/repo" "$c/box" "$entry" "$c/first.txt")
  bg_pids+=("$first")
  common_c=$(git_clean -C "$c/repo" rev-parse --path-format=absolute --git-common-dir)
  lock_file=$common_c/ita-gate-box.lock
  held=0
  for _ in $(seq 1 100); do
    if [[ -s $lock_file ]] && kill -0 "$first" 2>/dev/null; then held=1; break; fi
    sleep 0.05
  done
  RUN_SLEEP=0
  run_repo "$c/repo" "$c/box" "$entry" "$c/second.txt"
  second_rc=$?
  kill "$first" 2>/dev/null || :
  wait "$first" 2>/dev/null || :
  if [[ $kind == shipped && $held == 1 && $second_rc == 75 ]]; then
    ok 'lock exclusive — shipped script refuses concurrent run with exit 75'
  elif [[ $kind == mutant && $held == 1 && $second_rc != 75 ]]; then
    ok 'lock removed — concurrent run is not refused'
  else
    bad "lock $kind — expected named concurrency verdict (rc=$second_rc held=$held)"
  fi
done

# (d) A fresh box with map copying removed exposes MAP mismatch at the stub.
d=$LAB/case-map
mkdir -p "$d"
clone_fixture "$d/repo"
map_call='  copy_corpora_map "$source" "$box" "$common" || return 65'
mutate "$d/repo/scripts/dev" "$d/repo/scripts/dev-mutant.sh" "$map_call" \
  '  : # corpus map copy removed' || bad 'corpus map mutant was not created'
if [[ -x $d/repo/scripts/dev-mutant.sh ]]; then
  run_repo "$d/repo" "$d/box" scripts/dev-mutant.sh "$d/out.txt"
  run_rc=$?
  expected_map=$(map_hash "$d/repo")
  actual_map=$(stub_value MAP "$d/out.txt")
  if (( run_rc == 0 )) && [[ $actual_map != "$expected_map" ]]; then
    ok 'corpora-map copy removed — MAP mismatch accuses'
  else
    bad 'corpora-map copy removed — MAP mismatch was not observed'
  fi
fi

# (e) Removing env cleanup leaks the caller's target directory into the stub.
e=$LAB/case-env
mkdir -p "$e"
clone_fixture "$e/repo"
env_block='    # GATE_BOX_ENV_CLEAN
    env -u CARGO_TARGET_DIR -u CARGO_BUILD_TARGET_DIR -u ART -u TRANSCRIPT \
      "./$run_script" "$@"'
mutate "$e/repo/scripts/dev" "$e/repo/scripts/dev-mutant.sh" "$env_block" \
  '    env "./$run_script" "$@"' || bad 'env cleanup mutant was not created'
if [[ -x $e/repo/scripts/dev-mutant.sh ]]; then
  run_repo "$e/repo" "$e/box" scripts/dev-mutant.sh "$e/out.txt"
  if [[ $(stub_value CTD "$e/out.txt") == "$LAB/caller-target" ]]; then
    ok 'env cleanup removed — caller CARGO_TARGET_DIR is visible'
  else
    bad 'env cleanup removed — caller CARGO_TARGET_DIR stayed hidden'
  fi
fi

# (f) Removing the internal scrub leaks the lock helper's state into the
# script, where a nested `scripts/dev gates` would skip its own lock.
f=$LAB/case-scrub
mkdir -p "$f"
clone_fixture "$f/repo"
scrub_block='    # GATE_BOX_INTERNAL_SCRUB — see the direct branch above.
    unset ITA_GATE_BOX ITA_GATE_BOX_LOCKED ITA_GATE_BOX_SOURCE \
      ITA_GATE_BOX_TARGET_SHA ITA_GATE_BOX_COMMON ITA_GATE_BOX_WAIT'
mutate "$f/repo/scripts/dev" "$f/repo/scripts/dev-mutant.sh" "$scrub_block" \
  '    : # internal scrub removed' || bad 'internal scrub mutant was not created'
if [[ -x $f/repo/scripts/dev-mutant.sh ]]; then
  run_repo "$f/repo" "$f/box" scripts/dev-mutant.sh "$f/out.txt"
  if [[ $(stub_value LOCKED "$f/out.txt") == 1 ]]; then
    ok 'internal scrub removed — the lock state reaches the script'
  else
    bad 'internal scrub removed — the lock state stayed hidden'
  fi
fi

say 'summary'
if (( failed )); then
  printf 'RESULT: FAIL (lab kept at %s)\n' "$LAB"
  exit 1
fi
printf 'RESULT: PASS (lab kept at %s)\n' "$LAB"
exit 0
