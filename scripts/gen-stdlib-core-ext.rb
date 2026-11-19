#!/usr/bin/env ruby
# W11 (Tapioca open world): mechanical harvest of the instance methods the
# Ruby STANDARD LIBRARY adds to the core receiver chain when a program
# requires it. A default gem (`json`, `set`, `date`, `pathname`, ...) never
# appears in a `Gemfile.lock`, so Tapioca writes no RBI for it, yet
# `require "json"` gives every Integer `to_json`. The Tapioca open-world
# E0101 path (`check.rs`, `core_unknown_is_conclusive` under
# `TapiocaEvidence`) reads this file so a stdlib extension stays silent.
#
# Method: under `--disable-gems` (bundled and installed gems are out of
# reach, exactly as for `gen-core-inventory.rb`), snapshot every public,
# protected and private instance method of each chain class, then require
# every feature of the standard library in its OWN forked child (one
# feature's side effects never leak into the next) and print each name
# that appeared. A feature that adds `method_missing` or
# `respond_to_missing?` to the chain is printed as `Class#*`: it answers
# every name.
#
#   ruby --disable-gems scripts/gen-stdlib-core-ext.rb > \
#     crates/itaruby_semantic/declarations/stdlib_core_ext.txt
#
# The output is versioned and embedded via include_str!. Not a
# hand-maintained list: every line comes from the runtime's own
# reflection, after a real `require`.
abort("must run with 'ruby --disable-gems' (Gem is defined)") if defined?(Gem)

require "rbconfig"

CHAIN = %w[
  Integer Float String Symbol Array Hash NilClass TrueClass FalseClass
  Object BasicObject
].freeze

# Every helper lives on this module, never at top level: a top-level
# `def` is a private `Object` method, and this script's own names would
# otherwise be harvested as stdlib extensions.
module Harvest
  module_function

  def surface(name)
    klass = Object.const_get(name)
    (klass.instance_methods + klass.private_instance_methods).map(&:to_s)
  end

  def features
    dirs = [RbConfig::CONFIG["rubylibdir"], RbConfig::CONFIG["rubyarchdir"]]
    dirs.flat_map do |dir|
      Dir.glob("**/*.{rb,bundle,so}", base: dir).map { |f| f.sub(/\.(rb|bundle|so)\z/, "") }
    end.uniq.sort
  end

  # Each feature is required in its own forked child. A feature that
  # raises midway is still harvested: whatever it defined before the
  # raise is real at runtime for a program that rescues the LoadError.
  # A require that never returns (a console, a debugger) is killed after
  # a timeout and named on stderr — never silently dropped.
  def harvest(feature, before)
    reader, writer = IO.pipe
    pid = fork do
      reader.close
      $stdout.reopen(File::NULL)
      $stderr.reopen(File::NULL)
      $stdin.reopen(File::NULL)
      begin
        require feature
      rescue Exception # rubocop:disable Lint/RescueException
        nil
      end
      added = CHAIN.flat_map do |name|
        (surface(name) - before.fetch(name)).map { |m| "#{name}##{m}" }
      end
      # Every top-level MODULE the feature defined, with its public and
      # protected instance surface (ancestors included): what a bare
      # top-level `include M` hands every object. `@M` alone records a
      # module whose surface is empty (`FileUtils`: all `module_function`,
      # so private).
      (Object.constants - BEFORE_CONSTS).sort.each do |c|
        mod = begin
          Object.const_get(c)
        rescue Exception # rubocop:disable Lint/RescueException
          next
        end
        next unless mod.instance_of?(Module) && mod.name == c.to_s
        added << "@#{c}"
        mod.public_instance_methods.concat(mod.protected_instance_methods).each { |m| added << "@#{c}##{m}" }
      end
      writer.write(added.join("\n"))
      writer.close
      exit!(0)
    end
    writer.close
    deadline = Time.now + 20
    until Process.wait(pid, Process::WNOHANG)
      if Time.now > deadline
        Process.kill(:KILL, pid)
        Process.wait(pid)
        warn "timeout: #{feature}"
        return []
      end
      sleep 0.02
    end
    reader.read.split("\n")
  ensure
    reader&.close
  end
end

BEFORE = CHAIN.to_h { |name| [name, Harvest.surface(name)] }.freeze
BEFORE_CONSTS = Object.constants.freeze
FEATURES = Harvest.features
lines = FEATURES.flat_map { |f| Harvest.harvest(f, BEFORE) }
lines = lines.map { |l| l.end_with?("#method_missing", "#respond_to_missing?") ? l.sub(/#.*/, "#*") : l }
# A name `Object` (or `BasicObject`) gained reaches every chain class, so it
# is printed once on its owner, never repeated per class.
shared = lines.filter_map { |l| l.split("#", 2)[1] if l.start_with?("Object#", "BasicObject#") }.to_h { |m| [m, true] }
lines = lines.reject { |l| !l.start_with?("Object#", "BasicObject#", "@") && shared[l.split("#", 2)[1]] }.uniq.sort

puts <<~HEADER
  # itaruby stdlib core-extension inventory (W11) — GENERATED FILE, do not
  # hand-edit. One `Class#method` line per instance method a standard
  # library feature adds to a core receiver class when required (`Class#*`:
  # the feature adds `method_missing`/`respond_to_missing?`, so every name
  # answers). `@Module#method` lines: the public instance surface of each
  # top-level module a feature defines (`@Module` alone: empty surface).
  # Read only by the Tapioca open-world E0101 path.
  # Regenerate with:
  #
  #   ruby --disable-gems scripts/gen-stdlib-core-ext.rb > crates/itaruby_semantic/declarations/stdlib_core_ext.txt
  #
  # Generated with Ruby #{RUBY_VERSION} (#{RUBY_PLATFORM}), #{FEATURES.size} features.
HEADER
puts lines
