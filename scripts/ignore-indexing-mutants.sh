#!/usr/bin/env bash
# Two-sided proof for H4 (2026-10-20): `sorbet/config` `--ignore`d files
# are read as SUPPRESSION-ONLY evidence (`IgnoredFiles`), never dropped,
# never checked, never allowed to close a name. Every mutant removes ONE
# load-bearing decision and must be ACCUSED by a NAMED test in
# crates/itaruby/tests/sorbet_ignore_indexing.rs, which asserts the exact
# historical diagnostic the mutant re-emits.
#
#   IG-A  ignored files are dropped from indexing again (the ita-9d4
#         behavior)
#         -> ignored_only_constant_resolves_without_e0104 must fail:
#            `unresolved constant `SearchClient`` comes back
#   IG-B  `project_index` itself is built with ignored files merged like
#         project code (the ignored-aware index becomes the real one), so
#         a fixture stub CLOSES the name it shadows
#         -> ignored_fixture_stub_never_closes_a_gem_class must fail:
#            `Faraday.get` accuses against the stub
#   IG-C  ignored files are checked like project files
#         -> nothing_is_reported_inside_an_ignored_path must fail
#   IG-D  the openness pass is not run
#         -> ignored_reopening_opens_the_project_class must fail:
#            `undefined method `from_initializer` for `Widget`` returns
#   IG-E  `check_const_ref` stops consulting the ignored-aware index
#         -> ignored_only_constant_resolves_without_e0104 must fail
#   IG-F  the LSP stops routing ignored files to `IgnoredFiles`
#         -> lsp_honors_ignore_like_the_cli must fail
#
# Builds/tests go to $ROOT/target (AGENTS.md: the 2026-09-17 stale-binary
# and cross-worktree rmeta defects).
set -uo pipefail

cd "$(dirname "$0")/.."
ROOT=$PWD
MAIN=crates/itaruby/src/main.rs
IDX=crates/itaruby_semantic/src/index.rs
CHK=crates/itaruby_semantic/src/check.rs
SRV=crates/itaruby_server/src/main_loop.rs
BAK=$ROOT/target/ignore-indexing-mutants.orig
LOG=$ROOT/target/ignore-indexing-mutants-test.txt

fail=0
ok()  { printf 'PASS %s\n' "$1"; }
bad() { printf 'FAIL %s\n' "$1"; fail=1; }

mkdir -p "$BAK"
for f in "$MAIN" "$IDX" "$CHK" "$SRV"; do cp "$f" "$BAK/$(basename "$f")"; done
restore() {
  for f in "$MAIN" "$IDX" "$CHK" "$SRV"; do cp "$BAK/$(basename "$f")" "$f"; done
}
trap restore EXIT

compiles=0
failing=
# Sets `compiles`/`failing` in the CALLER's shell; never call it in a
# command substitution. `--no-fail-fast` keeps a later suite's control
# from silently not running.
run_suite() {
  CARGO_TARGET_DIR="$ROOT/target" cargo test -p itaruby --no-fail-fast \
    --test sorbet_ignore_indexing --test sorbet_ignore_cli >"$LOG" 2>&1
  if grep -q 'could not compile' "$LOG"; then compiles=0; else compiles=1; fi
  failing=$(grep -oE '^test [a-z0-9_]+ \.\.\. FAILED' "$LOG" | awk '{print $2}' | sort -u)
}

inject() {
  NEEDLE=$2 REPL=$3 python3 - "$1" <<'PY'
import os, sys
path = sys.argv[1]
src = open(path).read()
needle, repl = os.environ['NEEDLE'], os.environ['REPL']
n = src.count(needle)
if n != 1:
    print(f'INVALIDO: anchor matched {n} times in {path}', file=sys.stderr)
    sys.exit(2)
open(path, 'w').write(src.replace(needle, repl))
PY
}

# mutant <label> <file> <needle> <replacement> <must-fail test> <rationale>
mutant() {
  local label=$1 file=$2 needle=$3 repl=$4 expect=$5 why=$6
  printf '\n--- %s: %s\n' "$label" "$why"
  restore
  if ! inject "$file" "$needle" "$repl"; then
    bad "$label INVALIDO — anchor not found, mutant never injected"
    restore
    return
  fi
  run_suite
  if (( ! compiles )); then
    bad "$label did not compile — a mutant that cannot run proves nothing"
  elif [[ -z $failing ]]; then
    bad "$label BLIND — compiles and the suite is green: the control is missing, not the change safe"
  elif grep -qx "$expect" <<<"$failing"; then
    ok "$label accused by $expect ($(tr '\n' ' ' <<<"$failing" | sed 's/ $//'))"
  else
    bad "$label expected $expect to fail, got: $(tr '\n' ' ' <<<"$failing")"
  fi
  restore
}

echo '--- baseline (shipped source)'
run_suite
if (( ! compiles )) || [[ -n $failing ]]; then
  echo "FAIL baseline is not green: $failing"
  exit 1
fi
ok 'baseline green'

mutant IG-A "$MAIN" \
  '                    ignored_out.push(entry.path().to_path_buf());
' \
  '' \
  ignored_only_constant_resolves_without_e0104 \
  'ignored files are dropped from indexing again (ita-9d4)'

mutant IG-B "$IDX" \
  '    build_index(db, false)' \
  '    build_index(db, true)' \
  ignored_fixture_stub_never_closes_a_gem_class \
  'an ignored-only definition merges like project code and closes the name'

mutant IG-C "$MAIN" \
  '                    ignored_out.push(entry.path().to_path_buf());
                    continue;' \
  '                    ignored_out.push(entry.path().to_path_buf());' \
  nothing_is_reported_inside_an_ignored_path \
  'ignored files are checked like project files'

mutant IG-D "$IDX" \
  '        apply_ignored_openness(db, &mut index);' \
  '' \
  ignored_reopening_opens_the_project_class \
  'a project class an ignored file reopens stays closed'

mutant IG-E "$CHK" \
  '        if let Some(wide) = crate::index::ignored_aware_index(self.db) {' \
  '        if let Some(wide) = None::<&crate::ProjectIndex> {' \
  ignored_only_constant_resolves_without_e0104 \
  'E0104 stops consulting the ignored-aware index'

mutant IG-F "$SRV" \
  '        if project_root.is_ignored(&path) {' \
  '        if false {' \
  lsp_honors_ignore_like_the_cli \
  'the LSP indexes ignored files as project code'

echo
echo '--- restore and prove the shipped source is byte-identical'
restore
for f in "$MAIN" "$IDX" "$CHK" "$SRV"; do
  cmp -s "$f" "$BAK/$(basename "$f")" && ok "$f restored byte-identical" || bad "$f NOT restored"
done
run_suite
if (( ! compiles )) || [[ -n $failing ]]; then bad "post-restore suite not green: $failing"; fi
trap - EXIT
rm -rf "$BAK"

if (( fail )); then
  echo 'RESULT: FAIL (a mutant was blind, unaccused, or the source was not restored)'
  exit 1
fi
echo 'RESULT: PASS (every mutant accused by its named test, source restored byte-identical)'
