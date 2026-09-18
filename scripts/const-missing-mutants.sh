#!/usr/bin/env bash
# Two-sided proof for the `const_missing` fix (check.rs `check_const_ref`).
# The fix is suppression-only, so the mutation that matters is REMOVING the
# suppression: the false positive must come back, and only there.
#
#   MUT-A  the suppression call is cut
#          -> the hook fixture must false-positive again (the fix is what
#             silences it) AND the two controls must be unaffected
#   MUT-B  the suppression stops checking WHOSE hook it is (any namespace
#          with a resolvable parent silences)
#          -> the sibling-namespace control must go silent, proving the
#             ownership check is load-bearing and not decoration
#
# Builds go to $ROOT/target: measuring a binary another target-dir produced
# is the 2026-09-17 stale-binary defect (AGENTS.md).
set -uo pipefail

cd "$(dirname "$0")/.."
ROOT=$PWD
SRC=crates/itaruby_semantic/src/check.rs
ITA=$ROOT/target/release/ita
FIX=testdata/const_missing
BAK=$ROOT/target/const-missing-check.rs.orig

fail=0
ok()  { printf 'PASS %s\n' "$1"; }
bad() { printf 'FAIL %s\n' "$1"; fail=1; }

mkdir -p "$ROOT/target"
cp "$SRC" "$BAK"
restore() { cp "$BAK" "$SRC"; }
trap restore EXIT

build() { cargo build --release --locked --target-dir "$ROOT/target" >/dev/null 2>&1; }
diags() { "$ITA" check "$FIX/$1" 2>&1 | grep -cE 'error\[|warning\['; }

echo '--- baseline (shipped source)'
build || { echo 'FAIL baseline build'; exit 1; }
[[ $(diags hook_silences_qualified_ref.rb) == 0 ]] || bad 'baseline: hook fixture must be silent'
[[ $(diags bare_ref_inside_hook_namespace_silent.rb) == 0 ]] || bad 'baseline: bare ref inside the hook namespace must be silent'
[[ $(diags no_hook_still_warns.rb) == 1 ]] || bad 'baseline: no-hook control must warn'
[[ $(diags sibling_namespace_hook_still_warns.rb) == 1 ]] || bad 'baseline: sibling control must warn'
[[ $(diags bare_ref_outside_hook_namespace_still_warns.rb) == 1 ]] || bad 'baseline: nested bare ref must still warn'
[[ $(diags grandparent_qualified_still_warns.rb) == 1 ]] || bad 'baseline: grandparent-qualified control must warn'
if (( fail )); then echo 'ABORT: baseline is not the shipped behavior'; exit 1; fi
ok 'baseline two-sided (both hook shapes silent, all four controls still warn)'

echo '--- MUT-A: cut the suppression'
restore
ruby -e '
  src = File.read(ARGV[0])
  needle = "        if self.const_missing_namespace(scope, path) {"
  abort("MUT-A: anchor not found") unless src.include?(needle)
  File.write(ARGV[0], src.sub(needle, "        if false {"))
' "$SRC" || bad 'MUT-A could not be injected'
if build; then
  h=$(diags hook_silences_qualified_ref.rb)
  n=$(diags no_hook_still_warns.rb)
  s=$(diags sibling_namespace_hook_still_warns.rb)
  if [[ $h == 1 && $n == 1 && $s == 1 ]]; then
    ok 'MUT-A the false positive returns without the fix, controls unchanged'
  else
    bad "MUT-A expected hook=1 nohook=1 sibling=1, got hook=$h nohook=$n sibling=$s"
  fi
else
  bad 'MUT-A build failed — mutant proves nothing'
fi

echo '--- MUT-B: stop checking whose hook it is'
restore
ruby -e '
  src = File.read(ARGV[0])
  needle = "        matches!(\n            self.index.lookup_singleton(pid, \"const_missing\"),\n            MethodLookup::Found(..)\n        )"
  abort("MUT-B: anchor not found") unless src.include?(needle)
  File.write(ARGV[0], src.sub(needle, "        let _ = pid;\n        true"))
' "$SRC" || bad 'MUT-B could not be injected'
if build; then
  s=$(diags sibling_namespace_hook_still_warns.rb)
  n=$(diags no_hook_still_warns.rb)
  if [[ $s == 0 || $n == 0 ]]; then
    ok "MUT-B controls go silent (sibling=$s no-hook=$n) — the ownership check is what scopes the fix"
  else
    bad 'MUT-B controls still warn — the ownership check is not load-bearing'
  fi
else
  bad 'MUT-B build failed — mutant proves nothing'
fi

echo '--- MUT-C: cut the bare-reference (cref) branch'
# The bare branch was MISSING in the first version of this fix and the
# result was a live false positive on a program MRI runs clean (review
# 2026-09-17). Removing it must bring that false positive straight back,
# and must leave every qualified case untouched.
restore
ruby -e '
  src = File.read(ARGV[0])
  needle = "            None => scope.last().and_then(|inner| self.index.resolve_const(&[], inner)),"
  abort("MUT-C: anchor not found") unless src.include?(needle)
  File.write(ARGV[0], src.sub(needle, "            None => None,"))
' "$SRC" || bad 'MUT-C could not be injected'
if build; then
  b=$(diags bare_ref_inside_hook_namespace_silent.rb)
  q=$(diags hook_silences_qualified_ref.rb)
  if [[ $b == 1 && $q == 0 ]]; then
    ok 'MUT-C the bare-reference false positive returns, qualified path unaffected'
  else
    bad "MUT-C expected bare=1 qualified=0, got bare=$b qualified=$q"
  fi
else
  bad 'MUT-C build failed — mutant proves nothing'
fi

echo '--- restore and prove the shipped source is byte-identical'
restore
cmp -s "$SRC" "$BAK" && ok 'check.rs restored byte-identical' || bad 'check.rs NOT restored'
build || bad 'rebuild of the shipped source failed'
[[ $(diags hook_silences_qualified_ref.rb) == 0 ]] || bad 'post-restore: hook fixture must be silent again'
[[ $(diags sibling_namespace_hook_still_warns.rb) == 1 ]] || bad 'post-restore: sibling control must warn again'
trap - EXIT
rm -f "$BAK"

if (( fail )); then echo 'RESULT: FAIL (const_missing mutants)'; exit 1; fi
echo 'RESULT: PASS (const_missing mutants)'
