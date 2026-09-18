#!/usr/bin/env ruby
# frozen_string_literal: true

require 'json'
require 'fileutils'

# mine.rb — produces bug/fix PAIRS from the git history of a public Ruby repo.
#
# The main branch holds only the bugs that survived to main — dead paths. What
# a type checker is actually for is catching the regression at PR time. This
# script turns history into that benchmark: for every type-shaped bug fix it
# emits one JSONL record naming the buggy PARENT revision and the FIX
# revision, so replay.sh can run a checker on both and judge it two-sidedly.
#
#   ruby mine.rb CLONE_PATH ID [--out PATH] [--limit N] [--signal LIST]
#               [--max-hunks K] [--max-hunk-lines L] [--max-files F]
#
#   CLONE_PATH  a git clone (partial --filter=blob:none clones are fine; the
#               log walk fetches blobs lazily, newest first — `git backfill`
#               on the clone first makes big walks local and fast)
#   ID          repo id used for the default output path candidates/<id>.jsonl
#   --limit     cap of candidates per repo, newest first (default 300)
#
# Signals, in priority order (see process_commit — add yours there):
#   message           commit subject or merged-PR title (the first body line
#                     of a merge commit) mentions the classic Ruby
#                     exception/arity vocabulary
#   rescue-removal    the diff REMOVES a `rescue NoMethodError|NameError|
#                     ArgumentError` (a band-aid being taken off a wound)
#   single-token-edit a line whose ONLY change is a method-name rename or one
#                     positional argument added/removed
#
# --signal LIST mines only the named signals (comma-separated; default: all).
#
# Dedupe: a merged PR shows up twice with identical hunks — the merge commit
# and the PR tip commit. One record survives (the descriptive, non-`Merge`
# subject; else the newest) and the dropped fix_sha is recorded under "also"
# in the kept record.
#
# Output: one JSON object per line:
#   {id, fix_sha, parent_sha, subject, files:[...],
#    hunks:[{path, old_start, new_start, removed:[...], added:[...],
#            truncated}],
#    signal, files_truncated, hunks_truncated}
#   ("also": [sha, ...] appears exactly when an identical-hunk duplicate was
#    folded into this record)
#
# Only .rb files under production paths (app/, lib/, the Rails gem dirs)
# count; test/ and spec/ paths are excluded by pathspec AND re-filtered here
# (a test fix is not a production bug). Exit codes: 0 mined, 2 nothing to
# mine from (clone missing / no production paths), like gauntlet-gates.sh.
# A git promisor-fetch failure mid-walk leaves valid candidates but prints a
# loud TRUNCATED warning (rerunning resumes the fetch — fetched blobs stay).

def die(msg)
  warn "mine: #{msg}"
  exit 2
end

usage = 'usage: mine.rb CLONE_PATH ID [--out PATH] [--limit N] [--max-hunks K] [--max-hunk-lines L] [--max-files F]'
clone_path = ARGV.shift
id = ARGV.shift
die(usage) unless clone_path && id
die("not a git clone: #{clone_path}") unless File.directory?(File.join(clone_path, '.git'))

out_path = File.join(__dir__, 'candidates', "#{id}.jsonl")
limit = 300
signals_filter = nil
max_hunks = 24
max_files = 50
max_hunk_lines = 60

args = ARGV.dup
until args.empty?
  arg = args.shift
  flag, value = arg.split('=', 2)
  value ||= args.shift or die(usage)
  case flag
  when '--out'             then out_path = value
  when '--limit'           then limit = value.to_i
  when '--max-hunks'       then max_hunks = value.to_i
  when '--max-hunk-lines'  then max_hunk_lines = value.to_i
  when '--signal'          then signals_filter = value.split(',').map(&:strip)
  when '--max-files'       then max_files = value.to_i
  else die(usage)
  end
end

KNOWN_SIGNALS = %w[message rescue-removal single-token-edit].freeze
if signals_filter
  unknown = signals_filter - KNOWN_SIGNALS
  die("unknown signal(s) #{unknown.join(', ')} — known: #{KNOWN_SIGNALS.join(', ')}") unless unknown.empty?
end

# --- which production paths does this repo actually have? --------------------
# Extend the candidate list when a new repo shape shows up; the intersection
# with the clone's top level decides, so extra names here are harmless.
PRODUCTION_DIRS = %w[
  app lib activerecord activesupport actionpack actionview actionmailer
  activejob activestorage actioncable actionmailbox actiontext railties
].freeze
dirs = PRODUCTION_DIRS.select { |d| File.directory?(File.join(clone_path, d)) }
die("no production paths (#{PRODUCTION_DIRS.join(' ')}) under #{clone_path}") if dirs.empty?

# git pathspec: the production dirs, minus test/spec trees at any depth.
pathspecs = dirs + [':(glob)**/test/**', ':(glob)**/spec/**']

MESSAGE_RE = /NoMethodError|undefined method|NameError|uninitialized constant|ArgumentError|wrong number of arguments|undefined local variable/i.freeze
RESCUE_RE = /\brescue\s+(NoMethodError|NameError|ArgumentError)\b/.freeze
IDENT_RE = /\A[a-zA-Z_][a-zA-Z0-9_]*[?!]?\z/.freeze
ARG_SHELL_RE = /\A[()\s]*\z/.freeze
COMMA_ARG_RE = /\A,\s*\S.*\z|\A\S.*,\z/.freeze
BAD_SEGMENTS = %w[test spec].freeze

production_rb_path = lambda do |path|
  path.end_with?('.rb') && path.split('/').none? { |seg| BAD_SEGMENTS.include?(seg) }
end

# classify_line_pair: nil, or :method_rename / :arg_change when the ONLY
# difference between the removed and the added line is at one change point
# (identical prefix and suffix around it) and that point is a method-name or
# argument-list boundary:
#   both mids are identifiers after a `.` or `def `  -> method rename
#   one mid is empty/paren-shell, the other identifier-shaped, and the
#     change sits on the identifier run after a `.`, `def `, or `alias `
#     (covers trailing-char typo fixes like `totall` -> `total`) -> rename
#   one mid is empty/paren-shell, the other is a comma-led or comma-trailed
#     argument fragment inside a call                 -> positional arg change
classify_line_pair = lambda do |removed, added|
  a = removed.strip
  b = added.strip
  return nil if a.empty? || b.empty? || a == b

  min = [a.size, b.size].min
  p = (0...min).find { |i| a[i] != b[i] } || min
  s = 0
  s += 1 while s < min - p && a[a.size - 1 - s] == b[b.size - 1 - s]
  mid_a = a[p...a.size - s]
  mid_b = b[p...b.size - s]
  return nil if mid_a == mid_b

  prefix = a[0, p]
  defish = a.start_with?('def ', 'alias ')
  if mid_a.match?(IDENT_RE) && mid_b.match?(IDENT_RE) &&
     (prefix.end_with?('.') || prefix.match?(/def\s+\z/))
    return :method_rename
  end
  arg_shell = mid_a.empty? || mid_b.empty? ||
              mid_a.match?(ARG_SHELL_RE) || mid_b.match?(ARG_SHELL_RE)
  if arg_shell
    delta = mid_a.empty? || mid_a.match?(ARG_SHELL_RE) ? mid_b : mid_a
    other = delta.equal?(mid_a) ? mid_b : mid_a
    before = a[0...p]
    on_method_name = before.match?(/\.[\w?!]*\z/) ||
                     (defish && before.match?(/(?:def|alias)\s+[\w?!]*\z/))
    if delta.match?(IDENT_RE) && on_method_name
      return :method_rename
    end
    if prefix.include?('(') && (delta.match?(COMMA_ARG_RE) || delta.match?(IDENT_RE)) &&
       other.match?(ARG_SHELL_RE)
      return :arg_change
    end
  end
  nil
end

# --- stream `git log -p` newest first ----------------------------------------
cmd = [
  'git', '-C', clone_path, 'log', '--no-color', '-p', '-U0', '--no-renames',
  '--diff-merges=first-parent',
  '--pretty=format:%x01%H%x01%P%x01%s%x01%b%x02', '--'
] + pathspecs

records = []
signal_counts = Hash.new(0)
scanned = 0
started = Process.clock_gettime(Process::CLOCK_MONOTONIC)

io = IO.popen(cmd, 'r', external_encoding: Encoding::BINARY)
cur = nil

process_commit = lambda do |c|
  scanned += 1
  parents = c[:parents].split
  parent_sha = parents[0]
  return if parent_sha.nil? || c[:files].empty?

  pr_title = parents.size == 2 ? c[:body].find { |l| !l.strip.empty? } : nil
  message_hit = "#{c[:subject]}\n#{pr_title}" =~ MESSAGE_RE

  rescue_removal = c[:hunks].any? do |h|
    h[:removed].count { |l| l =~ RESCUE_RE } > h[:added].count { |l| l =~ RESCUE_RE }
  end
  single_token_edit = c[:hunks].any? do |h|
    h[:removed].any? { |r| h[:added].any? { |a| classify_line_pair.call(r, a) } }
  end

  signal =
    if message_hit    then 'message'
    elsif rescue_removal then 'rescue-removal'
    elsif single_token_edit then 'single-token-edit'
    else return
    end
  return if signals_filter && !signals_filter.include?(signal)

  records << {
    'id' => "#{id}-#{records.size + 1}-#{c[:sha][0, 8]}",
    'fix_sha' => c[:sha],
    'parent_sha' => parent_sha,
    'subject' => c[:subject],
    'files' => c[:files].first(max_files),
    'hunks' => c[:hunks].first(max_hunks).map do |h|
      {
        'path' => h[:path], 'old_start' => h[:old_start], 'new_start' => h[:new_start],
        'removed' => h[:removed].first(max_hunk_lines),
        'added' => h[:added].first(max_hunk_lines),
        'truncated' => h[:removed].size > max_hunk_lines || h[:added].size > max_hunk_lines
      }
    end,
    'signal' => signal,
    'files_truncated' => c[:files].size > max_files,
    'hunks_truncated' => c[:hunks].size > max_hunks
  }
  signal_counts[signal] += 1
end

git_status = nil
begin
  io.each_line("\n") do |raw|
    line = raw.chomp.force_encoding(Encoding::UTF_8).scrub
    if line.getbyte(0) == 0x01
      process_commit.call(cur) if cur
      fields = line.split("\x01", -1)
      body = fields[4] || ''
      body_done = body.sub!("\x02", '')
      cur = {
        sha: fields[1], parents: fields[2], subject: fields[3].to_s,
        body: body_done ? [] : [body], body_done: body_done ? true : nil,
        files: [], files_seen: {}, hunks: [], file: nil, deleted: false, hunk: nil
      }
    elsif cur
      if !cur[:body_done] && line.include?("\x02")
        line = line.sub("\x02", '')
        cur[:body_done] = true
        cur[:body] << line unless line.empty?
      elsif cur[:body_done]
        case line
        when /\Adiff --git /
          path = line[/ b\/(.*)\z/, 1].to_s.sub(/\t.*\z/, '').sub(/"\z/, '').sub(/\A"/, '')
          cur[:file] = production_rb_path.call(path) ? path : nil
          cur[:deleted] = false
          cur[:hunk] = nil
        when /\A\+\+\+ /
          cur[:deleted] = line.include?('/dev/null')
        when /\A@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/
          next unless cur[:file]
          cur[:hunk] = { path: cur[:file], old_start: Regexp.last_match(1).to_i,
                         new_start: Regexp.last_match(2).to_i, removed: [], added: [] }
          cur[:hunks] << cur[:hunk]
          unless cur[:files_seen][cur[:file]]
            cur[:files_seen][cur[:file]] = true
            cur[:files] << cur[:file]
          end
        when /\A-/
          cur[:hunk][:removed] << line[1..] if cur[:hunk] && cur[:hunk][:removed].size < max_hunk_lines
        when /\A\+/
          cur[:hunk][:added] << line[1..] if cur[:hunk] && cur[:hunk][:added].size < max_hunk_lines
        end
      elsif cur[:body].size < 40
        cur[:body] << line
      end
    end
    if records.size >= limit
      io.close
      break
    end
  end
  process_commit.call(cur) if cur
ensure
  io.close unless io.closed?
  Process.wait(io.pid) rescue nil
  git_status = $?.exitstatus rescue nil
end
if git_status && git_status != 0 && records.size < limit
  warn format('mine: WARNING git log exited %d after %d commits — history walk TRUNCATED; '\
              'candidates are valid but fewer than the cap (rerun to resume fetching)', git_status, scanned)
end

# --- dedupe identical-hunk twins (merge commit + PR tip) ----------------------
MERGE_SUBJECT_RE = /\AMerge (pull request|branch|remote-tracking branch)\b/.freeze
hunk_key = lambda do |r|
  r['hunks'].map { |h| JSON.generate([h['path'], h['removed'], h['added']]) }.sort.join("\x1F")
end
pre_dedupe = records.size
by_key = {}
records.each do |r|
  key = hunk_key.call(r)
  kept = by_key[key]
  if kept.nil?
    by_key[key] = r
  elsif MERGE_SUBJECT_RE =~ kept['subject'] && MERGE_SUBJECT_RE !~ r['subject']
    r['also'] = ([kept['fix_sha']] + kept.fetch('also', [])).uniq
    by_key[key] = r
  else
    kept['also'] = (kept.fetch('also', []) + [r['fix_sha']]).uniq
  end
end
records = by_key.values

seconds = (Process.clock_gettime(Process::CLOCK_MONOTONIC) - started).round(1)
FileUtils.mkdir_p(File.dirname(out_path))
File.open(out_path, 'w') { |f| records.each { |r| f.puts(JSON.generate(r)) } }

warn format(
  'mine: %s — %d commits scanned, %d candidates (%d after dedupe; %s) -> %s in %.1fs',
  id, scanned, pre_dedupe, records.size,
  signal_counts.map { |k, v| "#{k}=#{v}" }.join(' '),
  out_path, seconds
)
exit 0
