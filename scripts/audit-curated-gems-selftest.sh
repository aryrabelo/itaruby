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
# parse error, a hidden lib/ file, `extend` on a chain receiver, a missing
# gem dir, an unreadable subtree built at run time) and its EXACT expected
# output, or its abort, is asserted. The generator probes in
# scripts/gen-stdlib-core-ext-fixture/ are required through
# `ITA_STDLIB_FEATURES` (the generator's feature-list override) with the
# fixture dir on `-I`: a redefined `method_missing`/`respond_to_missing?`
# on a chain class must print `Class#*`, and a generator that prints the
# right lines and then fails (`crash_gen.rb`) must be refused.
#
# Silence side: the `clean` fixture (a non-chain class with mixins, alias,
# `class << self`, `extend` on a project class, an exe/ script; it loads
# under MRI) must print only its header, and a probe that adds one
# ordinary method must print exactly that name.
#
# Precondition: Ruby >= 3.4.2, the runtime the inventory was generated with
# (the generator aborts on anything older). Without it nothing here can be
# judged, so the script exits 2 - the gauntlet reports a SKIP and the run
# is incomplete, never green.
#
#   0  both sides proved
#   1  a fixture printed anything but its expected lines
#   2  no Ruby >= 3.4.2 on PATH: nothing judged
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

if ! ruby_ver=$(ruby -e 'print RUBY_VERSION' 2>/dev/null); then
  echo 'no ruby on PATH (need >= 3.4.2)'
  exit 2
fi
if ! ruby -e 'exit((RUBY_VERSION.split(".").map(&:to_i) <=> [3, 4, 2]) >= 0)'; then
  echo "ruby $ruby_ver is older than 3.4.2, the inventory's generator runtime"
  exit 2
fi

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

# Hidden files under lib/ are Ruby a require can load: a dot-skipping glob
# once read this fixture as zero files.
expect_audit d8-hidden "### d8-hidden (1 lib .rb, 0 exe Ruby, 0 exe skipped)
  chain_def${TAB}String#d8_hidden${TAB}lib/.d8_hidden.rb:2"

# `extend` reaches the singleton surface wherever its receiver is a chain
# class: a constant receiver, a chain body, an indirect receiver.
expect_audit d9-extend "### d9-extend (1 lib .rb, 0 exe Ruby, 0 exe skipped)
  singleton_call${TAB}String.singleton extend${TAB}lib/d9.rb:2
  singleton_body_call${TAB}Integer.singleton extend${TAB}lib/d9.rb:4
  chain_call_indirect${TAB}Object.const_get(:Hash).extend(D9)${TAB}lib/d9.rb:6"

# expect_abort <label> <dir> <stderr glob>: non-zero exit, nothing on
# stdout, the reason on stderr. A missing or partly unreadable gem tree
# once read as a clean, empty report.
expect_abort() {
  local out err rc
  out=$(ruby "$AUDIT" "$2" 2>/dev/null); rc=$?
  err=$(ruby "$AUDIT" "$2" 2>&1 >/dev/null)
  # shellcheck disable=SC2053 # $3 is a glob on purpose
  if [[ $rc -ne 0 && -z $out && $err == $3 ]]; then ok "audit $1 aborts"
  else bad "audit $1 (exit $rc, stdout '$out', stderr '$err')"
  fi
}
expect_abort d10-missing "$AFIX/d10-missing" '*d10-missing is not a readable gem directory*'

# An unreadable subtree, built at run time (git keeps no modes). Measured
# after the chmod: a process that still lists a 0o000 dir (root) cannot
# build the shape, and says so instead of passing.
lab=$(mktemp -d "${TMPDIR:-/tmp}/audit-selftest.XXXXXX") || { bad 'audit d11: no temp dir'; exit 1; }
mkdir -p "$lab/d11-unreadable/lib/locked"
printf 'class String\n  def d11_hidden = 1\nend\n' >"$lab/d11-unreadable/lib/locked/d11.rb"
chmod 000 "$lab/d11-unreadable/lib/locked"
if ls "$lab/d11-unreadable/lib/locked" >/dev/null 2>&1; then
  echo "NOTE audit d11-unreadable not built: this process reads a 0o000 directory"
else
  expect_abort d11-unreadable "$lab/d11-unreadable" '*cannot read *d11-unreadable*'
fi
chmod 755 "$lab/d11-unreadable/lib/locked"
rm -r "$lab"

# expect_gen <features> <expected inventory lines>: the generator must
# exit 0 too - expected lines followed by a crash are no inventory.
expect_gen() {
  local got rc
  got=$(ITA_STDLIB_FEATURES=$1 ruby --disable-gems -I "$GFIX" "$GEN" 2>/dev/null)
  rc=$?
  got=$(grep -v '^#' <<<"$got")
  if [[ $rc -eq 0 && $got == "$2" ]]; then ok "gen $1"
  else
    bad "gen $1 (exit $rc)"
    diff <(printf '%s\n' "$2") <(printf '%s\n' "$got") | sed 's/^/    /'
  fi
}

expect_gen mm_probe "Integer#*"
expect_gen rtm_probe "String#*"
expect_gen plain_probe "Integer#plain_probe_name"

# The exit status is part of the verdict: a generator that prints exactly
# the right inventory and then fails (the stand-in `crash_gen.rb`) must be
# refused - the guard on `expect_gen` itself. The subshell keeps its own
# `failed`/`GEN`, so the probe's expected FAIL never reaches the verdict.
# shellcheck disable=SC2030,SC2031
if (failed=0; GEN=$GFIX/crash_gen.rb; expect_gen plain_probe "Integer#crash_probe_name" >/dev/null; exit "$failed"); then
  bad 'gen crash_gen accepted: expect_gen ignores the generator exit status'
else
  ok 'gen crash_gen refused (exit status checked)'
fi

# shellcheck disable=SC2031 # only the probe subshell above wrote its own copy
exit $failed
