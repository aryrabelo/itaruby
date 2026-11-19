#!/usr/bin/env bash
# Two-sided proof for the two instruments whose output SILENCES E0101 in
# the Tapioca open world (AGENTS.md: the instrument that produces the
# evidence gets the same two-sided treatment):
#   - scripts/audit-curated-gems.rb  (the `CURATED_GEMS` names)
#   - scripts/gen-stdlib-core-ext.rb (declarations/stdlib_core_ext.txt)
# A name either one misses is a false positive waiting in the checker.
#
# Accusation side: each defect fixture under
# scripts/audit-curated-gems-fixture/dN-* encodes one shape the audit once
# missed or misreported (instance_exec, the `alias` keyword, `class <<
# self`, top-level mixins, receiver `eval`, extensionless exe/ scripts, a
# parse error) and its EXACT expected output is asserted. The generator
# probes in scripts/gen-stdlib-core-ext-fixture/ are required through
# `ITA_STDLIB_FEATURES` (the generator's feature-list override) with the
# fixture dir on `-I`: a redefined `method_missing`/`respond_to_missing?`
# on a chain class must print `Class#*`.
#
# Silence side: the `clean` fixture (a non-chain class with mixins, alias,
# `class << self`, an exe/ script) must print only its header, and a
# probe that adds one ordinary method must print exactly that name.
#
#   0  both sides proved
#   1  a fixture printed anything but its expected lines
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1
ROOT=$PWD
AUDIT=$ROOT/scripts/audit-curated-gems.rb
AFIX=$ROOT/scripts/audit-curated-gems-fixture
GEN=$ROOT/scripts/gen-stdlib-core-ext.rb
GFIX=$ROOT/scripts/gen-stdlib-core-ext-fixture
TAB=$'\t'

failed=0
ok()  { printf 'PASS %s\n' "$1"; }
bad() { printf 'FAIL %s\n' "$1"; failed=1; }

# expect_audit <fixture> <expected stdout>
expect_audit() {
  local got
  got=$(ruby "$AUDIT" "$AFIX/$1" 2>&1)
  local rc=$?
  if [[ $rc -eq 0 && $got == "$2" ]]; then ok "audit $1"
  else
    bad "audit $1 (exit $rc)"
    diff <(printf '%s\n' "$2") <(printf '%s\n' "$got") | sed 's/^/    /'
  fi
}

expect_audit clean "### clean (1 lib .rb, 1 exe Ruby, 0 exe skipped)"

expect_audit d1-instance-exec "### d1-instance-exec (1 lib .rb, 0 exe Ruby, 0 exe skipped)
  chain_call${TAB}String.instance_exec${TAB}lib/d1.rb:2"

expect_audit d2-alias "### d2-alias (1 lib .rb, 0 exe Ruby, 0 exe skipped)
  chain_def${TAB}String#d2_shout${TAB}lib/d2.rb:3
  toplevel_def${TAB}d2_say${TAB}lib/d2.rb:7"

expect_audit d3-singleton-class "### d3-singleton-class (1 lib .rb, 0 exe Ruby, 0 exe skipped)
  singleton_def${TAB}String.singleton#d3_build${TAB}lib/d3.rb:5
  singleton_body_call${TAB}String.singleton include${TAB}lib/d3.rb:6
  singleton_def${TAB}main.singleton#d3_helper${TAB}lib/d3.rb:11"

expect_audit d4-toplevel-mixin "### d4-toplevel-mixin (1 lib .rb, 0 exe Ruby, 0 exe skipped)
  chain_body_call${TAB}Object include${TAB}lib/d4.rb:7
  chain_body_call${TAB}Object prepend${TAB}lib/d4.rb:8
  chain_body_call${TAB}Object define_method${TAB}lib/d4.rb:9
  singleton_body_call${TAB}main.singleton extend${TAB}lib/d4.rb:10"

expect_audit d5-receiver-eval "### d5-receiver-eval (1 lib .rb, 0 exe Ruby, 0 exe skipped)
  eval${TAB}Kernel.eval(\"class Integer; def d5_kernel = 1; end\")${TAB}lib/d5.rb:2
  eval${TAB}binding.eval(\"def d5_binding = 1\")${TAB}lib/d5.rb:3"

expect_audit d6-exe-shebang "### d6-exe-shebang (1 lib .rb, 1 exe Ruby, 1 exe skipped)
  toplevel_def${TAB}d6_exe_helper${TAB}exe/d6-tool:4
  skipped_non_ruby_exe${TAB}\"#!/bin/sh\"${TAB}exe/d6-wrapper"

# A parse error aborts: non-zero exit, the file named on stderr, and no
# report on stdout (a partial parse would have read as two chain defs).
out=$(ruby "$AUDIT" "$AFIX/d7-parse-error" 2>/dev/null); rc=$?
err=$(ruby "$AUDIT" "$AFIX/d7-parse-error" 2>&1 >/dev/null)
if [[ $rc -ne 0 && -z $out && $err == *"parse error in "*"d7-parse-error/lib/d7.rb:"* ]]; then ok "audit d7-parse-error aborts"
else bad "audit d7-parse-error (exit $rc, stdout '$out', stderr '$err')"
fi

# expect_gen <features> <expected inventory lines>
expect_gen() {
  local got
  got=$(ITA_STDLIB_FEATURES=$1 ruby --disable-gems -I "$GFIX" "$GEN" 2>/dev/null | grep -v '^#')
  if [[ $got == "$2" ]]; then ok "gen $1"
  else
    bad "gen $1"
    diff <(printf '%s\n' "$2") <(printf '%s\n' "$got") | sed 's/^/    /'
  fi
}

expect_gen mm_probe "Integer#*"
expect_gen rtm_probe "String#*"
expect_gen plain_probe "Integer#plain_probe_name"

exit $failed
