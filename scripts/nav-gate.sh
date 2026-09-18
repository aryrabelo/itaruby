#!/usr/bin/env bash
# nav-gate.sh — judges a scripts/nav-oracle.rb fixture against `ita definition`.
# Pure comparator: it never boots anything itself, it only reads a JSONL
# fixture and asks ita about each call site in it. gate e in
# scripts/gauntlet-gates.sh owns deciding whether a fixture is even
# obtainable on this machine (that's the "sem app bootável => skip" half);
# this script owns the judging half.
#
# Usage: nav-gate.sh [FIXTURE_PATH] [APP_ROOT]
#   FIXTURE_PATH  JSONL fixture (default: NAV_ORACLE_JSONL env, else the
#                 repo's own scripts/navfixture/oracle.jsonl — always
#                 present, no client app needed).
#   APP_ROOT      base directory relative `call_path`/`def_path` entries
#                 are resolved against (a fixture harvested with
#                 nav-oracle.rb's --relative stores paths this way so it is
#                 committable — see scripts/navfixture/workload.rb).
#                 Default: the fixture's own directory, which is correct
#                 for the repo fixture; override with NAV_APP_ROOT for an
#                 external one.
#
# Exit codes mirror gauntlet-gates.sh's own scheme:
#   0  every app_def pair ita answered, it answered correctly
#   1  at least one app_def pair got a wrong answer (or ita errored on one)
#   2  no usable fixture — nothing to judge
#
# Only app_def pairs (definition is a real `def` inside the app) can fail
# this gate. external/generated/c_method pairs are legitimately ambiguous —
# a Rails DSL method's source_location often points at the generator, not
# the line a human would click to; the runtime and RubyMine disagree there
# too — so an ita answer there is only logged as "to check", never a
# failure. Invariant #1 extended to navigation: no answer is always fine,
# a wrong one never is.
#
# Object#method(:x).source_location has no column, so the comparison below
# is (path, line) only — ita's reported column is real but there is no
# ground truth here to check it against.
set -uo pipefail

ROOT=${ROOT:-$(cd "$(dirname "$0")/.." && pwd)}
ITA=${ITA:-$ROOT/target/release/ita}
FIXTURE=${1:-${NAV_ORACLE_JSONL:-$ROOT/scripts/navfixture/oracle.jsonl}}
ART=${ART:-$ROOT/target/gauntlet}
MISMATCHES=$ART/nav-mismatches.txt
mkdir -p "$ART"

if [[ ! -s $FIXTURE ]]; then
  echo "SKIP: no fixture at $FIXTURE" >&2
  exit 2
fi
if [[ ! -x $ITA ]]; then
  echo "FAIL: no ita binary at $ITA" >&2
  exit 1
fi

APP_ROOT=${2:-${NAV_APP_ROOT:-$(cd "$(dirname "$FIXTURE")" && pwd)}}

# ponytail: the comparison logic lives in embedded ruby, not hand-rolled bash
# JSON parsing — ruby is already a hard dependency of this exact feature
# (nav-oracle.rb needs it too), and this way there is exactly one process
# spawn per fixture line (one `ita definition` call) instead of three.
ruby - "$FIXTURE" "$ITA" "$MISMATCHES" "$APP_ROOT" <<'RUBY'
require 'json'
require 'open3'

fixture_path, ita, mismatches_path, app_root = ARGV

# call_path/def_path are absolute unless the fixture was harvested with
# nav-oracle.rb's --relative, in which case they're relative to APP_ROOT
# (see scripts/navfixture/workload.rb) — this is what makes a fixture
# committable.
def resolve(app_root, path)
  return path if path.nil? || path.start_with?('/')

  File.join(app_root, path)
end

def realpath(path)
  return nil if path.nil?

  File.realpath(path)
rescue StandardError
  path
end

match = unknown = mismatch = error = to_check = informational = 0

File.open(mismatches_path, 'w') do |mismatches|
  File.foreach(fixture_path) do |line|
    line = line.strip
    next if line.empty?

    rec = JSON.parse(line, symbolize_names: true)
    call_path = resolve(app_root, rec[:call_path])
    call_site = "#{call_path}:#{rec[:call_line]}:#{rec[:call_col]}"
    answer, status = Open3.capture2(ita, 'definition', call_site)
    answer = answer.strip
    exit_status = status.exitstatus

    if rec[:category] != 'app_def'
      if exit_status.zero? && answer != 'unknown'
        to_check += 1
        mismatches.puts "TO-CHECK [#{rec[:category]}] #{call_site} -> #{answer} " \
                         "(oracle: #{resolve(app_root, rec[:def_path])}:#{rec[:def_line]}, informational only, never fails the gate)"
      else
        informational += 1
      end
      next
    end

    if exit_status != 0
      error += 1
      mismatches.puts "ERROR #{call_site} exit=#{exit_status} #{answer}"
      next
    end

    if answer == 'unknown'
      unknown += 1
      next
    end

    # answer is "<path>:<line>:<col>" from the CLI contract; split from the
    # right so a path containing ':' (rare on Unix, but possible) survives.
    parts = answer.split(':')
    ans_line = parts[-2]
    ans_path = parts[0..-3].join(':')

    if realpath(ans_path) == realpath(resolve(app_root, rec[:def_path])) && ans_line == rec[:def_line].to_s
      match += 1
    else
      mismatch += 1
      mismatches.puts "MISMATCH #{call_site} ita=#{answer} oracle=#{resolve(app_root, rec[:def_path])}:#{rec[:def_line]} " \
                       "(#{rec[:method]} on #{rec[:defined_class]})"
    end
  end
end

puts "nav-gate: app_def match=#{match} unknown=#{unknown} mismatch=#{mismatch} error=#{error} | " \
     "informational(external/generated/c_method)=#{informational} to_check=#{to_check}"
exit((mismatch.positive? || error.positive?) ? 1 : 0)
RUBY
status=$?

if [[ $status -eq 1 ]]; then
  echo "FAIL: nav gate found app_def pair(s) disagreeing with the runtime oracle (see $MISMATCHES)" >&2
fi
exit "$status"
