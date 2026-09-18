#!/usr/bin/env ruby
# frozen_string_literal: true

# The inference bench: does itaruby find real Ruby bugs with ZERO annotations,
# and does it stay silent on dynamic code that is correct at runtime?
#
# Ground truth here is not a human's opinion and not the other checker's
# output — it is MRI. Every `accuse` case is executed and must really raise
# at the line it is blamed on; every `silent` case is executed and must
# really exit 0. A fixture that does not do what its manifest row claims is
# a broken fixture, and this oracle says so before it judges any checker
# (AGENTS.md: a probe that fails for the wrong reason proves nothing, and
# the wrong reason is usually the fixture).
#
# Scope, stated so no reader infers parity (this is NOT a parity benchmark):
#   * Every fixture is self-contained, stdlib-only, zero gems. Neither side's
#     declaration world is exercised: itaruby's curated `declarations/` and
#     Sorbet's Tapioca/RBI gem pipeline are both out of scope by construction.
#     Nothing here says anything about either tool on a real gem-bearing app.
#   * Sorbet runs at the sigil each fixture pins (`# typed: true`), under the
#     `srb` version pinned in the manifest. A different version is a SKIP,
#     never a silent re-measurement.
#   * When Sorbet reports on code that runs clean, the only claim this bench
#     makes is "Sorbet cannot prove this without an annotation" (a `sig` or
#     an RBI declaration) — never "Sorbet cannot do this", and never "Sorbet
#     does not resolve this". The `with_annotations` leg is what earns even
#     that narrower sentence: the SAME program (byte-identical, sha-checked)
#     plus the annotation a human or Tapioca would write, on which Sorbet is
#     measured clean. A row with no such leg is reported, never claimed.
#   * Both directions are published. Cases where Sorbet catches what itaruby
#     misses are first-class rows in the ledger, not omissions.
#
# Isolation: each case owns a directory, and each tool is pointed at that
# directory alone. Every diagnostic's path is verified to belong to the case
# being judged — a shared `--dir` mixes results, so a row that reads on
# another case's file is a failure, not a match.
#
#   ruby scripts/inference-bench.rb [--sorbet] [--json out.jsonl] [--ita PATH]
#
# exit 0  every row matched its manifest
# exit 1  a divergence (or a broken fixture, or a leaked path)
# exit 2  matched everything it could, but a leg was skipped (no/mismatched srb)

require 'json'
require 'digest'
require 'open3'

ROOT = File.expand_path('..', __dir__)
CASES_DIR = File.join(ROOT, 'testdata', 'gauntlet_inference')
MANIFEST = File.join(ROOT, 'scripts', 'inference-bench.jsonl')

opts = { sorbet: false, json: nil, ita: File.join(ROOT, 'target', 'release', 'ita') }
args = ARGV.dup
until args.empty?
  case (a = args.shift)
  when '--sorbet' then opts[:sorbet] = true
  when '--json' then opts[:json] = args.shift
  when '--ita' then opts[:ita] = args.shift
  else abort "unknown argument: #{a}"
  end
end

failed = []
skipped = []
rows = []

def sha256(path) = Digest::SHA256.file(path).hexdigest

# --- MRI, the ground truth -------------------------------------------------
# Runs the fixture in a subprocess and reports what really happened. The call
# site is the LAST frame of the backtrace that belongs to the fixture: for a
# NoMethodError that is the only frame, for an ArgumentError raised inside the
# callee it is the caller, which is the line a checker blames.
RUNNER = <<~'RUBY'
  target = ARGV[0]
  begin
    load target
    print JSON.generate({ 'clean' => true })
  rescue Exception => e # rubocop:disable Lint/RescueException
    frames = (e.backtrace || []).select { |f| f.start_with?(target) }
    line = frames.last&.slice(/:(\d+):/, 1)&.to_i
    print JSON.generate({ 'clean' => false, 'error' => e.class.name, 'line' => line })
  end
RUBY

def runtime_truth(path)
  out, err, st = Open3.capture3('ruby', '-rjson', '-e', RUNNER, path)
  return { 'broken' => "ruby runner produced no verdict (#{st.exitstatus}): #{err.lines.first}" } if out.empty?

  JSON.parse(out)
end

# --- itaruby ---------------------------------------------------------------
DIAG = /\A(?<path>.+?):(?<line>\d+):(?<col>\d+): (?<sev>error|warning)\[(?<code>E\d+)\]: (?<msg>.*)\z/

# Returns [diags, error_or_nil]. A binary that crashes, panics, or writes to
# stderr produces NO parsed diagnostics, which is byte-identical to "clean"
# if the caller only reads stdout — so a broken build would score as perfect
# silence on every silence row. Exit status and stderr are part of the
# measurement, not noise.
ITA_OK_STATUSES = [0, 1].freeze # 0 = nothing found, 1 = diagnostics found

def ita_diags(ita, path)
  out, err, st = Open3.capture3(ita, 'check', path)
  diags = out.lines.filter_map do |l|
    m = DIAG.match(l.chomp)
    next unless m

    { 'path' => m[:path], 'line' => m[:line].to_i, 'code' => m[:code], 'sev' => m[:sev] }
  end
  problem = nil
  unless ITA_OK_STATUSES.include?(st.exitstatus)
    problem = "itaruby exited #{st.exitstatus.inspect} (crash/panic?): #{err.lines.first&.chomp}"
  end
  problem ||= "itaruby wrote to stderr: #{err.lines.first.chomp}" unless err.strip.empty?
  problem ||= 'itaruby exited 1 (diagnostics found) but printed none this parser understood' if st.exitstatus == 1 && diags.empty?
  [diags, problem]
end

# --- Sorbet ----------------------------------------------------------------
SORBET_DIAG = /\A(?<path>\S+\.rbi?):(?<line>\d+): (?<msg>.*?) https:\/\/srb\.help\/(?<code>\d+)\z/

def sorbet_diags(dir)
  out, err, _st = Open3.capture3('srb', 'tc', '--no-config', '--silence-dev-message', '--dir', dir)
  (out + err).lines.filter_map do |l|
    m = SORBET_DIAG.match(l.chomp)
    next unless m

    { 'path' => m[:path], 'line' => m[:line].to_i, 'code' => m[:code] }
  end
end

def sorbet_version
  out, = Open3.capture3('srb', '--version')
  out[/Sorbet typechecker (\S+)/, 1]
rescue Errno::ENOENT
  nil
end

# --- manifest --------------------------------------------------------------
abort "missing manifest: #{MANIFEST}" unless File.exist?(MANIFEST)
manifest = File.readlines(MANIFEST).reject { |l| l.strip.empty? || l.start_with?('#') }.map { |l| JSON.parse(l) }
by_id = manifest.to_h { |m| [m['id'], m] }

on_disk = Dir.children(CASES_DIR).select { |c| File.directory?(File.join(CASES_DIR, c)) }.sort
missing_rows = on_disk - by_id.keys
orphan_rows = by_id.keys - on_disk
# A fixture nobody judges is the bench's blindest spot, so this is announced
# where a reader sees it, not merely counted into the exit code.
unless missing_rows.empty?
  failed << "coverage: case directories with no manifest row: #{missing_rows.join(', ')}"
  puts "FAIL #{failed.last}"
end
unless orphan_rows.empty?
  failed << "coverage: manifest rows with no case directory: #{orphan_rows.join(', ')}"
  puts "FAIL #{failed.last}"
end

pinned_srb = manifest.first&.dig('sorbet_version')
srb = opts[:sorbet] ? sorbet_version : nil
if opts[:sorbet]
  if srb.nil?
    skipped << 'sorbet leg: `srb` not runnable on this machine'
    opts[:sorbet] = false
  elsif srb != pinned_srb
    skipped << "sorbet leg: srb #{srb} != pinned #{pinned_srb} (re-pin the manifest deliberately)"
    opts[:sorbet] = false
  end
end

unless File.executable?(opts[:ita])
  failed << "no itaruby binary at #{opts[:ita]}"
  puts "FAIL #{failed.last}"
  exit 1
end
puts "ita binary: #{opts[:ita]} sha256=#{sha256(opts[:ita])[0, 12]} mtime=#{File.mtime(opts[:ita]).iso8601}"
puts "sorbet:     #{srb || '(leg not run)'}"
puts

manifest.each do |row|
  id = row['id']
  dir = File.join(CASES_DIR, id)
  plain = File.join(dir, 'no_annotations', 'plain.rb')
  problems = []

  unless File.exist?(plain)
    failed << "#{id}: missing #{plain}"
    next
  end

  # sigil pinned per fixture, per the manifest
  sigil = File.open(plain, &:readline).chomp
  problems << "sigil is #{sigil.inspect}, manifest pins #{row['sigil'].inspect}" if sigil != row['sigil']

  # the fairness leg must be the SAME program, or it proves nothing
  ann_dir = File.join(dir, 'with_annotations')
  has_ann = Dir.exist?(ann_dir)
  if has_ann != !row['fairness'].nil?
    problems << (has_ann ? 'with_annotations/ exists but manifest declares no fairness leg' : 'manifest declares a fairness leg with no with_annotations/')
  elsif has_ann
    ann_plain = File.join(ann_dir, 'plain.rb')
    if !File.exist?(ann_plain)
      problems << 'fairness leg has no plain.rb'
    elsif sha256(ann_plain) != sha256(plain)
      problems << 'fairness leg is NOT the same program (sha mismatch) — it would prove nothing'
    end
  end

  # 1. MRI: what really happens
  truth = runtime_truth(plain)
  if truth['broken']
    problems << "runtime: #{truth['broken']}"
  elsif row['runtime']['clean']
    problems << "runtime: manifest says clean, MRI raised #{truth['error']} at line #{truth['line']}" unless truth['clean']
  elsif truth['clean']
    problems << "runtime: manifest says #{row['runtime']['error']} at #{row['runtime']['line']}, MRI exited 0 — the bug is not real"
  else
    problems << "runtime: manifest says #{row['runtime']['error']}, MRI raised #{truth['error']}" if truth['error'] != row['runtime']['error']
    problems << "runtime: manifest says line #{row['runtime']['line']}, MRI blamed #{truth['line']}" if truth['line'] != row['runtime']['line']
  end

  # 2. itaruby, path-verified, stderr and exit status included
  diags, ita_problem = ita_diags(opts[:ita], plain)
  problems << ita_problem if ita_problem
  leaked = diags.reject { |d| File.expand_path(d['path']) == File.expand_path(plain) }
  problems << "itaruby reported on another case's file: #{leaked.map { |d| d['path'] }.uniq.join(', ')}" unless leaked.empty?
  ita_expected = row['ita']
  if ita_expected['diags'].empty?
    problems << "itaruby: manifest expects silence, got #{diags.map { |d| "#{d['code']}@#{d['line']}" }.join(', ')}" unless diags.empty?
  else
    got = diags.map { |d| { 'code' => d['code'], 'line' => d['line'] } }
    want = ita_expected['diags'].map { |d| { 'code' => d['code'], 'line' => d['line'] } }
    problems << "itaruby: expected #{want.inspect}, got #{got.inspect}" if got != want
  end

  # 2b. POSITIVE CONTROL. Silence on correct code proves nothing on its own:
  # a checker blind to the whole surface is silent too. The control is the
  # same dynamic shape carrying a certain typo, so MRI must raise on it, and
  # what itaruby does there separates "correctly silent" from "blind". Its
  # expected verdict is recorded like any other — including `miss`, which is
  # what all three currently are.
  pc_diags = nil
  if row['positive_control']
    pc = File.join(dir, 'positive_control', 'plain.rb')
    if File.exist?(pc)
      pc_truth = runtime_truth(pc)
      if pc_truth['clean']
        problems << 'positive control does not raise under MRI — it controls for nothing'
      elsif pc_truth['error'] != row['positive_control']['runtime']['error']
        problems << "positive control: manifest says #{row['positive_control']['runtime']['error']}, MRI raised #{pc_truth['error']}"
      elsif pc_truth['line'] != row['positive_control']['runtime']['line']
        problems << "positive control: manifest says line #{row['positive_control']['runtime']['line']}, MRI blamed #{pc_truth['line']}"
      end
      pc_diags, pc_problem = ita_diags(opts[:ita], pc)
      problems << "positive control: #{pc_problem}" if pc_problem
      want = row['positive_control']['ita']['diags'].map { |d| { 'code' => d['code'], 'line' => d['line'] } }
      got = pc_diags.map { |d| { 'code' => d['code'], 'line' => d['line'] } }
      problems << "positive control: itaruby expected #{want.inspect}, got #{got.inspect}" if got != want
    else
      problems << 'manifest declares a positive control with no positive_control/plain.rb'
    end
  elsif Dir.exist?(File.join(dir, 'positive_control'))
    problems << 'positive_control/ exists but the manifest declares none'
  end

  # 3. Sorbet, path-verified against the FULL path (a basename check would
  # accept another case's plain.rb, which is exactly the leak being guarded)
  sorbet_got = nil
  if opts[:sorbet]
    no_ann = File.join(dir, 'no_annotations')
    sorbet_got = sorbet_diags(no_ann)
    expected_path = File.expand_path(plain)
    s_leaked = sorbet_got.reject { |d| File.expand_path(d['path'], no_ann) == expected_path }
    problems << "sorbet reported outside the case: #{s_leaked.map { |d| d['path'] }.uniq.join(', ')}" unless s_leaked.empty?
    want = row['sorbet']['diags'].map { |d| { 'line' => d['line'], 'code' => d['code'] } }
    got = sorbet_got.map { |d| { 'line' => d['line'], 'code' => d['code'] } }
    problems << "sorbet: expected #{want.inspect}, got #{got.inspect}" if got != want

    if row['fairness']
      after = sorbet_diags(ann_dir)
      if row['fairness']['sorbet_after'] == 'clean' && !after.empty?
        problems << "fairness: sorbet must be clean once declared, got #{after.map { |d| "#{d['code']}@#{d['line']}" }.join(', ')}"
      end
    end
  end

  verdict = problems.empty? ? 'OK' : 'FAIL'
  failed.concat(problems.map { |p| "#{id}: #{p}" })
  printf("%-4s %-34s %-9s ita:%-22s sorbet:%s\n", verdict, id, row['arm'],
         row['ita']['verdict'], opts[:sorbet] ? row['sorbet']['verdict'] : '(skipped)')
  # id-prefixed so a reader (and the selftest) can tell WHICH case accused,
  # never just that something did.
  problems.each { |p| puts "       #{id}: #{p}" }
  rows << row.merge('measured' => { 'runtime' => truth, 'ita' => diags, 'sorbet' => sorbet_got })
end

if opts[:json]
  File.write(opts[:json], rows.map { |r| JSON.generate(r) }.join("\n") + "\n")
  puts "\nwrote #{opts[:json]}"
end

# --- the scoreboard, both directions ---------------------------------------
# A verdict may be mixed (`hit_with_false_positive`): a checker that catches
# the real bug AND accuses a line that runs fine counts in both columns.
hit = ->(r, t) { r[t]['verdict'].start_with?('hit') }
fp  = ->(r, t) { r[t]['verdict'].include?('false_positive') }
puts "\n--- scoreboard (#{manifest.size} cases, zero annotations, zero gems, no gem RBIs) ---"
ita_only = manifest.select { |r| hit.(r, 'ita') && !hit.(r, 'sorbet') }
srb_only = manifest.select { |r| hit.(r, 'sorbet') && !hit.(r, 'ita') }
both_hit = manifest.select { |r| hit.(r, 'ita') && hit.(r, 'sorbet') }
ita_fp   = manifest.select { |r| fp.(r, 'ita') }
srb_fp   = manifest.select { |r| fp.(r, 'sorbet') }
both_quiet = manifest.select { |r| r['ita']['verdict'] == 'silent' && r['sorbet']['verdict'] == 'silent' }
show = ->(label, set) { puts format('%-46s %2d  %s', label, set.size, set.map { |r| r['id'] }.join(', ')) unless set.empty? }
show.('both prove the bug:', both_hit)
show.('itaruby proves it, sorbet does not:', ita_only)
show.('sorbet proves it, itaruby does not (our gaps):', srb_only)
show.('both correctly silent:', both_quiet)
show.('itaruby false positives (invariant #1 debt):', ita_fp)
pc_rows  = manifest.select { |r| r['positive_control'] }
pc_blind = pc_rows.reject { |r| r['positive_control']['ita']['verdict'] == 'hit' }
show.('silence rows carrying a positive control:', pc_rows)
show.('...whose control itaruby also misses (blind):', pc_blind)
unless pc_blind.empty?
  puts "\nLIMITATION, published: on those #{pc_blind.size} row(s) itaruby is silent on the"
  puts 'correct program AND silent on a typo'"'"'d variant of it that MRI really'
  puts 'raises on. That silence is FAIL-CLOSED (the receiver or the class is'
  puts 'open, so nothing is provable), NOT demonstrated inference of the dynamic'
  puts 'surface. Read those rows as "does not false-positive here", never as'
  puts '"understands this pattern".'
end
show.('sorbet needs an annotation (leg proven clean):', srb_fp.select { |r| r['fairness'] })
show.('sorbet false positive, no fairness leg filed:', srb_fp.reject { |r| r['fairness'] })

# --- the claim block ------------------------------------------------------
# Every number in the sentences below is DERIVED from the manifest, so a
# claim cannot drift away from what was measured. A single sentence can only
# name one sigil and one srb version, so rows that disagree about either
# would make the block a lie: that is a failure, not a formatting detail.
sigils = manifest.map { |r| r['sigil'] }.uniq
versions = manifest.map { |r| r['sorbet_version'] }.uniq
failed << "claim scope: rows disagree on the sigil (#{sigils.join(', ')}) — no single claim covers them" if sigils.size > 1
failed << "claim scope: rows disagree on the pinned srb version (#{versions.join(', ')})" if versions.size > 1
legs = srb_fp.count { |r| r['fairness'] }
puts "\n--- claim scope (quote this, not the table alone) ---"
puts "Fixtures:      #{manifest.size} self-contained files, one per case directory."
puts "Sigil:         every fixture pins #{sigils.join(', ')} on its first line."
puts "Sorbet:        srb #{versions.join(', ')}#{srb && srb != versions.first ? " (MISMATCH: ran #{srb})" : ''}."
puts 'RBI world:     zero gems, zero Tapioca/generated RBIs, zero itaruby'
puts '               curated declarations. Neither tool gets its declaration'
puts '               pipeline, so nothing here measures either tool on a real app.'
puts 'Timing:        NO timing, speed, or throughput claim is made or measured'
puts '               by this bench. It measures verdicts only.'
puts "Claim licensed: on #{legs} row(s), \"Sorbet cannot prove this without an"
puts '               annotation" — each re-runs sorbet on the SAME program'
puts '               (sha-checked) plus the sig/RBI a human or Tapioca would'
puts '               write, measured clean. Rows with no such leg are reported,'
puts '               never claimed. This is not a parity benchmark.'

if failed.any?
  puts "\nRESULT: FAIL (#{failed.size})"
  exit 1
end
if skipped.any?
  skipped.each { |s| puts "SKIP #{s}" }
  puts 'RESULT: PASS (incomplete)'
  exit 2
end
puts "\nRESULT: PASS"
