#!/usr/bin/env ruby
# frozen_string_literal: true

# judge.rb — the two-sided classifier of the bug-replay benchmark. One record
# (a mined bug/fix pair) plus the diagnostics each tool produced at the buggy
# PARENT revision and at the FIX revision, in, one classification out:
#
#   HIT        a diagnostic on a touched file at parent is GONE at fix
#              (the checker saw the bug and the fix silenced it)
#   MISS       silent on both sides (a false negative — acceptable under
#              Invariant #1, but it is the recall this benchmark measures)
#   NOISE      a diagnostic on a touched file at FIX that the parent did NOT
#              have — a potential false positive ON THE FIXED CODE. Flagged
#              loudly: this is the one thing this project must never do.
#   UNRELATED  diagnostics at parent that are still present, unchanged, at
#              fix — the tool is firing, but not on this bug.
#
# Precedence when several apply: NOISE > HIT > UNRELATED > MISS — a fix-side
# diagnostic is always the headline, and the per-file detail keeps the
# silenced evidence visible even under the NOISE label.
#
# "Same diagnostic" means same code+message (ita) or same message (srb),
# paired nearest-line-first; line drift between revisions does not make a
# chronic diagnostic look silenced. Whether the parent diagnostic landed on
# the fixed line +-3 is recorded separately as line_match (the "ideally" in
# the spec), never as part of the label.
#
# Usage:
#   ruby judge.rb --record REC.json \
#     --parent-diags P.jsonl --parent-root PARENT_DIR \
#     --fix-diags F.jsonl --fix-root FIX_DIR \
#     --tool ita|srb
#
# Prints one JSON object: {label, line_match, files:[...], counts:{...}}.
# Exit 0 always (a judgment, not a failure) — callers read the label.

require 'json'

def die(msg)
  warn "judge: #{msg}"
  exit 2
end

opts = {}
ARGV.each do |arg|
  m = arg.match(/\A--([^=]+)=(.*)\z/)
  die("unknown arg: #{arg}") unless m
  opts[m[1].tr('-', '_')] = m[2]
end
%w[record parent_diags parent_root fix_diags fix_root tool].each do |k|
  die("--#{k.tr('_', '-')} is required") unless opts[k]
end
die("--tool must be ita or srb") unless %w[ita srb].include?(opts['tool'])

record = JSON.parse(File.read(opts['record']))
touched = record['files'] || []
ANSI_RE = /\e\[[0-9;]*m/.freeze

# Parse one tool's diagnostics into {rel_path => [{line:, key:}]}.
diagnostics = lambda do |diags_path, root|
  by_file = Hash.new { |h, k| h[k] = [] }
  File.foreach(diags_path) do |raw|
    line = raw.scrub.gsub(ANSI_RE, '').rstrip
    next if line.empty?

    if opts['tool'] == 'ita'
      obj = JSON.parse(line)
      path = obj['path']
      next unless path
      path = path.delete_prefix("#{root}/") if root && !root.empty?
      by_file[path] << { line: obj['line'].to_i, key: "#{obj['code']} #{obj['message']}" }
    else
      # srb tc: "path:line: Message https://srb.help/7001" (context lines don't match and are skipped)
      m = line.match(/\A\s*([^:\s][^:]*?):(\d+):\s*(.+?)\s*\z/)
      next unless m
      key = m[3].sub(%r{\s*https://srb\.help/\d+\s*\z}, '')
      by_file[m[1]] << { line: m[2].to_i, key: key }
    end
  end
  by_file
end

parent = diagnostics.call(opts['parent_diags'], opts['parent_root'])
fix = diagnostics.call(opts['fix_diags'], opts['fix_root'])

# Pair parent diags to fix diags by key, nearest line first, so a surviving
# chronic diagnostic is never mistaken for a silenced one.
files_out = touched.map do |path|
  p_d = (parent[path] || []).sort_by { |d| d[:line] }
  f_d = (fix[path] || [])
  deleted_at_fix = f_d.empty? && path.then do |p|
    root = opts['fix_root']
    root.empty? ? !File.exist?(p) : !File.exist?(File.join(root, p))
  end
  unused = f_d.dup
  surviving = []
  silenced = []
  p_d.each do |d|
    idx = unused.each_index.min_by do |i|
      unused[i][:key] == d[:key] ? (unused[i][:line] - d[:line]).abs : Float::INFINITY
    end
    if idx && unused[idx][:key] == d[:key]
      surviving << d.merge(fix_line: unused[idx][:line])
      unused.delete_at(idx)
    else
      silenced << d
    end
  end
  { path: path, deleted_at_fix: deleted_at_fix,
    silenced: silenced, surviving: surviving, new: unused.sort_by { |d| d[:line] } }
end

counts = { silenced: 0, surviving: 0, new: 0 }
files_out.each do |f|
  counts[:silenced] += f[:silenced].size
  counts[:surviving] += f[:surviving].size
  counts[:new] += f[:new].size
end

# line_match: does any silenced diagnostic land within +-3 lines of a hunk's
# removed region (the line the fix actually touched)?
anchors = Hash.new { |h, k| h[k] = [] }
(record['hunks'] || []).each do |h|
  next unless h['path']
  lo = h['old_start'].to_i
  hi = lo + h.fetch('removed', []).size
  anchors[h['path']] << [lo, hi]
end
line_match = files_out.any? do |f|
  f[:silenced].any? do |d|
    anchors[f[:path]].any? { |lo, hi| d[:line] >= lo - 3 && d[:line] <= hi + 3 }
  end
end

label =
  if counts[:new].positive?      then 'NOISE'
  elsif counts[:silenced].positive? then 'HIT'
  elsif counts[:surviving].positive? then 'UNRELATED'
  else 'MISS'
  end

# class_match: does the subject's failure vocabulary predict the family of
# what was actually silenced? rails-6 taught why this matters: a HIT whose
# only silenced diagnostic is an E0104 while the subject says ArgumentError
# is silence-by-deleted-reference, not the fix the subject describes. The
# label stays HIT (a judgment, not a rejection); `class_match` is the caveat
# replay.sh prints as HIT?. srb messages carry no E-codes, so the check is
# ita-only (always true for srb). A subject matching no vocabulary carries
# no expectation, so class_match stays true there as well.
EXPECTED_FAMILIES = [
  [/NoMethodError|undefined method|undefined local/i, %w[E0101 E0104]],
  [/NameError|uninitialized constant/i,               %w[E0101 E0104]],
  [/ArgumentError|wrong number of arguments/i,        %w[E0102 E0103]],
].freeze
families = EXPECTED_FAMILIES.select { |re, _| re =~ record['subject'].to_s }
                           .flat_map(&:last).uniq
silenced_codes = files_out.flat_map { |f| f[:silenced] }
                          .filter_map { |d| d[:key][/\AE\d+/] }
class_match =
  if opts['tool'] != 'ita' || families.empty?
    true
  else
    silenced_codes.any? { |code| families.include?(code) }
  end

puts JSON.generate(
  label: label, line_match: line_match, class_match: class_match,
  expected_family: families.empty? ? nil : families,
  files: files_out, counts: counts,
  meta: { tool: opts['tool'], id: record['id'], fix_sha: record['fix_sha'],
          parent_sha: record['parent_sha'], signal: record['signal'] }
)
