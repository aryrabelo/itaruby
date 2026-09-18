#!/usr/bin/env ruby
# frozen_string_literal: true

# nav-oracle.rb — harvests real go-to-definition ground truth from a running
# Ruby/Rails process. It enables TracePoint(:call, :c_call) over a workload
# and, for every distinct (call site, method) pair, resolves the callee's
# canonical Object#method(:x).source_location — that IS the oracle:
# itaruby's `ita definition` must match it whenever it dares to answer
# (invariant #1 extended to navigation: no answer is fine, a wrong one
# never is — see AGENTS.md).
#
# Usage (run *inside* the target app, e.g. via `bin/rails runner` so Rails is
# already booted, or `bundle exec ruby -Ilib` for a plain app):
#
#   ruby nav-oracle.rb APP_ROOT [ENTRY] [--out PATH] [--max-app-def N] [--relative]
#
#   APP_ROOT         root of the app being probed. Used to classify app_def
#                    vs external, and as the base for a relative ENTRY.
#   ENTRY            a Ruby file `load`ed under the trace (relative to
#                    APP_ROOT, or absolute) — the actual workload: an app's
#                    test suite, a smoke script, whatever exercises real call
#                    sites. Omit it to just `Rails.application.eager_load!`
#                    a booted Rails app (weaker: eager_load defines methods,
#                    it barely calls any, so this mostly harvests boot noise
#                    — pass ENTRY when a real workload is available).
#   --out PATH       JSONL destination (default: nav-oracle.jsonl, cwd).
#   --relative       write call_path/def_path relative to APP_ROOT instead
#                     of absolute, for paths that fall under APP_ROOT (any
#                     path outside it — gem/stdlib — stays absolute; nothing
#                     else to make it relative to). Needed for a fixture
#                     that gets committed: an absolute path bakes the
#                     harvesting machine's home directory into the repo.
#   --max-app-def N  stop tracing once this many distinct app_def pairs are
#                    collected (default 300). caller_locations + a method
#                    lookup per traced call is not free; unbounded tracing
#                    over a whole test suite never ends.
#
# One JSON object per line:
#   call_path, call_line, call_col   where the call happened (col is a
#                                     best-effort regex match on the source
#                                     line — Ruby's own backtraces carry no
#                                     column, so this is the oracle's own
#                                     best guess, not ground truth)
#   method, defined_class            what was called, and on which class
#   def_path, def_line               method(:x).source_location — nil/nil
#                                     for C methods
#   category                         app_def | external | generated | c_method
#
# Categories:
#   app_def    definition is a real `def` inside APP_ROOT       -> must match ita
#   external   definition is a real file outside APP_ROOT       -> informational
#              (gem/stdlib)
#   generated  definition is "(eval)", or a known Rails-         -> informational
#              generated module (e.g. a `has_many`/attribute
#              method whose source_location points at the DSL's
#              generator, not the line the human wrote — the
#              runtime and RubyMine legitimately disagree here
#              too, so neither answer nor silence can fail here)
#   c_method   source_location is nil (C-implemented)            -> informational

require 'json'

def die(msg)
  warn "nav-oracle: #{msg}"
  exit 1
end

app_root_arg = ARGV.shift
die('usage: nav-oracle.rb APP_ROOT [ENTRY] [--out PATH] [--max-app-def N] [--relative]') unless app_root_arg

entry = nil
out_path = 'nav-oracle.jsonl'
max_app_def = 300
relative = false

args = ARGV.dup
until args.empty?
  arg = args.shift
  case arg
  when '--out'
    out_path = args.shift
    die('--out needs a value') unless out_path
  when '--max-app-def'
    value = args.shift
    die('--max-app-def needs a value') unless value
    max_app_def = Integer(value)
  when '--relative'
    relative = true
  else
    die("unexpected extra argument: #{arg}") if entry
    entry = arg
  end
end

app_root =
  begin
    File.realpath(app_root_arg)
  rescue StandardError
    die("APP_ROOT not found: #{app_root_arg}")
  end

oracle_self = File.realpath(__FILE__)

# ponytail: only the Rails-generated module names this project has actually
# seen so far — extend the list as new ones show up instead of guessing a
# broad heuristic that risks swallowing genuine app_def classes.
GENERATED_CLASS_RE = /Generated(Attribute|Association|FeatureMethods)Methods/.freeze

line_cache = {}
read_line = lambda do |path, lineno|
  cached = (line_cache[path] ||= begin
    File.readlines(path)
  rescue StandardError
    []
  end)
  cached[lineno - 1]
end

call_column = lambda do |path, lineno, method_name|
  src = read_line.call(path, lineno)
  next 1 unless src

  name = Regexp.escape(method_name.to_s)
  if (m = src.match(/\.#{name}\b/))
    m.begin(0) + 2 # 1-based, skip past the receiver '.'
  elsif (m = src.match(/\b#{name}\b/))
    m.begin(0) + 1
  else
    1
  end
end

classify = lambda do |def_path, defined_class|
  return :c_method if def_path.nil?
  return :generated if def_path == '(eval)' || def_path.include?('(eval)')
  return :generated if defined_class.to_s =~ GENERATED_CLASS_RE

  def_path.start_with?("#{app_root}/") ? :app_def : :external
end

relativize = lambda do |path|
  return path unless relative && path && path.start_with?("#{app_root}/")

  path.delete_prefix("#{app_root}/")
end

seen = {}
app_def_count = 0
records = []

tp = TracePoint.new(:call, :c_call) do |t|
  begin
    # :call enters a Ruby frame for the callee, so the real call site sits
    # two levels up from this block; :c_call has no such frame (the C
    # method never gets its own Ruby stack entry), so it is one level up.
    # (verified empirically — Ruby's own docs are silent on this offset.)
    call_loc = caller_locations(t.event == :call ? 2 : 1, 1)&.first
    call_path = call_loc&.absolute_path
    next if call_path.nil? || call_path == oracle_self

    klass = t.defined_class
    method_id = t.method_id
    next unless klass && method_id

    unbound =
      begin
        klass.instance_method(method_id)
      rescue NameError
        next
      end

    call_line = call_loc.lineno
    key = [call_path, call_line, klass.to_s, method_id].join('|')
    next if seen[key]
    seen[key] = true

    def_path, def_line = unbound.source_location
    category = classify.call(def_path, klass)

    records << {
      call_path: relativize.call(call_path),
      call_line: call_line,
      call_col: call_column.call(call_path, call_line, method_id),
      method: method_id.to_s,
      defined_class: klass.to_s,
      def_path: relativize.call(def_path),
      def_line: def_line,
      category: category.to_s
    }

    if category == :app_def
      app_def_count += 1
      tp.disable if app_def_count >= max_app_def
    end
  rescue StandardError
    next # a hostile call frame must never crash the workload it is observing
  end
end

tp.enable

begin
  if entry
    entry_path = entry.start_with?('/') ? entry : File.join(app_root, entry)
    load entry_path
  elsif defined?(Rails) && Rails.respond_to?(:application) && Rails.application
    Rails.application.eager_load!
  end
ensure
  tp.disable
end

File.open(out_path, 'w') { |f| records.each { |r| f.puts(JSON.generate(r)) } }

counts = records.group_by { |r| r[:category] }.transform_values(&:size)
warn "nav-oracle: #{records.size} pairs (#{counts.map { |k, v| "#{k}=#{v}" }.join(' ')}) -> #{out_path}"
