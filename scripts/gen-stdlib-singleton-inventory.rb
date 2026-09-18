#!/usr/bin/env ruby
# Singleton-track family (e): mechanical harvest of every SINGLETON
# method each stdlib namespace really answers — `FileUtils.mkdir_p`,
# `SecureRandom.uuid`, `Math.sqrt`, `Digest::MD5.hexdigest`, the
# `Kernel` module functions (`raise`, `require`, `Array`, `Integer`,
# `block_given?`) — straight from the Ruby runtime, in exactly the shape
# gen-stdlib-inventory.rb and gen-core-inventory.rb already use. NOT a
# hand-maintained list: the lib list comes from RbConfig's own lib dirs
# and every method line comes from the language's own reflection.
#
#   ruby --disable-gems scripts/gen-stdlib-singleton-inventory.rb > \
#     crates/itaruby_semantic/declarations/stdlib_singletons.txt
#
# One fresh subprocess per lib (`RbConfig.ruby --disable-gems`), so no
# require can pollute another's constant set. A subprocess that fails to
# require (LoadError, crash) contributes zero lines — never aborts the
# harvest.
#
# WHAT IS DELIBERATELY EXCLUDED, and why the exclusion lives HERE rather
# than in the consumer:
#
#   * every method `Module`/`Class` itself answers (`name`, `ancestors`,
#     `instance_methods`, `const_get`, `class_eval`, ...). Those are the
#     class-object surface `core.rs::kernel_object_singleton_method`
#     already models, and the checker already softens on them. Emitting
#     them would double the file for zero new knowledge and make the
#     "what does this file add?" question unanswerable.
#   * the core classes `core.rs` models as types (Integer, Float,
#     String, Symbol, Array, Hash, NilClass, TrueClass, FalseClass,
#     Object). Their surface is `declarations/core_inventory.txt`'s job;
#     two files claiming one namespace is how a filter rots.
#
# Both exclusions are properties of this file's CONTENT, so they are
# checked against the committed file by
# `crates/itaruby_semantic/tests/stdlib_singletons.rs` — which is what
# makes them survive a regeneration rather than depending on whoever
# runs the generator next.
#
# Guard: same as its two siblings. A Ruby older than the one this file
# was first generated with (3.4.2) would shrink the harvest and turn
# suppressed diagnostics back on downstream. Refuse instead.
abort("need Ruby >= 3.0 (generated with 3.4.2); found #{RUBY_VERSION}") if RUBY_VERSION.to_f < 3.0

abort(
  "must run with 'ruby --disable-gems' — RubyGems is loaded (Gem is " \
  "defined), so a harvested method could come from a gem instead of " \
  "the language"
) if defined?(Gem)

require 'rbconfig'

dirs = [RbConfig::CONFIG['rubylibdir'], RbConfig::CONFIG['rubyarchdir']].uniq
libs = {}
dirs.each do |dir|
  Dir.glob("#{dir}/**/*.rb").each do |f|
    libs[f.delete_prefix("#{dir}/").delete_suffix('.rb')] = true
  end
  Dir.glob("#{dir}/**/*.{so,bundle,dylib}").each do |f|
    name = f.delete_prefix("#{dir}/")
    libs[name.split('.').first] = true
  end
end

# The child diffs `Object.constants` around the require, so a namespace
# that already existed before it (Object, Kernel, Comparable, ...) is
# never attributed to a stdlib lib — with ONE deliberate exception,
# `Kernel`, which pre-exists every require and is precisely where the
# module functions this family exists for live. It is harvested from the
# base process below instead.
#
# WHICH REFLECTION CALL, and why not the obvious one. The first version
# used `singleton_methods(false)` — methods defined directly on the
# namespace object — and measurably missed the most wanted entry of the
# whole family: `SecureRandom.uuid` was absent, because SecureRandom
# gets its surface by EXTENDING `Random::Formatter`, so nothing is
# defined "directly" on it at all. The harvest is therefore
# `singleton_methods(true)` minus everything `Module` and `Class`
# themselves answer: that keeps `def self.x`, `extend self`,
# `module_function` AND `extend SomeModule` (all four spellings the
# singleton track resolves) while still excluding the class-object
# surface `core.rs::kernel_object_singleton_method` owns.
CHILD = <<~RB
  CLASS_OBJECT_SURFACE = (Module.methods | Class.methods).freeze
  CORE_TYPES = %w[
    Integer Float String Symbol Array Hash
    NilClass TrueClass FalseClass Object
  ].freeze
  pre = Object.constants
  begin
    require ARGV[0]
  rescue Exception
    exit 2
  end
  def emit(mod, path, out)
    return if CORE_TYPES.include?(path)
    (mod.singleton_methods(true) - CLASS_OBJECT_SURFACE).sort.each do |m|
      out << "\#{path}.\#{m}"
    end
  end
  def walk(mod, prefix, depth, seen, out)
    return if depth >= 8 || seen.include?(mod.object_id)
    seen << mod.object_id
    mod.constants(false).each do |c|
      path = prefix.empty? ? c.to_s : "\#{prefix}::\#{c}"
      begin
        v = mod.const_get(c)
      rescue NameError, LoadError
        next
      end
      next unless v.is_a?(Module)
      emit(v, path, out)
      walk(v, path, depth + 1, seen, out)
    end
  end
  out = []
  (Object.constants - pre).each do |c|
    v = Object.const_get(c) rescue nil
    next unless v.is_a?(Module)
    emit(v, c.to_s, out)
    walk(v, c.to_s, 0, [], out)
  end
  puts out
RB

results = {}
libs.keys.sort.each do |lib|
  lines = nil
  IO.popen([RbConfig.ruby, '--disable-gems', '-e', CHILD, lib],
           err: '/dev/null') do |io|
    lines = io.read.to_s.split("\n")
  end
  results[lib] = lines if $?.success? && !lines.empty?
end

# The namespaces that exist BEFORE any require — `Kernel`, `Math`,
# `Process`, `GC`, `Signal`, `ObjectSpace`, `Marshal`, ... The child's
# pre/post constant diff can never see them (that diff is what keeps a
# core name from being attributed to some stdlib lib), and they carry
# the single most common family-(e) receivers there are: `Kernel.rand`,
# `Kernel.raise`, `Kernel.require`, `Kernel.Array`, `Math.sqrt`,
# `Process.pid`. They are harvested here, from this process, under the
# lib name `<core>` — which is not a requirable lib and is therefore
# never matched by a project's `require`, exactly like the rest of this
# file's suppression-only contract.
CLASS_OBJECT_SURFACE = (Module.methods | Class.methods).freeze
CORE_TYPES = %w[
  Integer Float String Symbol Array Hash
  NilClass TrueClass FalseClass Object
].freeze
core = []
Object.constants.sort.each do |c|
  v = Object.const_get(c) rescue nil
  next unless v.is_a?(Module)
  next if CORE_TYPES.include?(c.to_s)
  (v.singleton_methods(true) - CLASS_OBJECT_SURFACE).sort.each do |m|
    core << "#{c}.#{m}"
  end
end
results['<core>'] = core unless core.empty?

puts <<~HEADER
  # itaruby stdlib SINGLETON method inventory (singleton-track family
  # (e)) — GENERATED FILE, do not hand-edit.
  #
  # One `<Namespace>.<method>` line per singleton method a stdlib
  # namespace really answers, mechanically harvested from the Ruby
  # runtime — never written by hand. The harvest is
  # `singleton_methods(true)` minus everything `Module`/`Class`
  # themselves answer, which is the set `def self.x`, `extend self`,
  # `module_function` and `extend SomeModule` produce: all four
  # spellings the singleton track resolves. (`singleton_methods(false)`
  # was the first attempt and missed `SecureRandom.uuid`, whose surface
  # arrives by extending `Random::Formatter`.)
  #
  # NO LIB COLUMN, unlike `stdlib_constants.txt`. That file gates on the
  # project actually `require`-ing the lib, because a constant that is
  # not loaded really is unresolved. This one is SUPPRESSION ONLY —
  # consumed by `index.rs::soften_not_found` on the singleton track,
  # where a hit turns a would-be `NotFound` into `Inconclusive` — so
  # gating it could only ever turn silence into a diagnostic on code
  # that runs. Ungated is invariant #1's direction, and it makes the
  # file a plain sorted set instead of 50k lib-attributed duplicates.
  #
  # Excluded by construction (see the generator's header for why): every
  # `Module`/`Class` method (`core.rs::kernel_object_singleton_method`'s
  # territory) and the ten core classes `core_inventory.txt` owns. Both
  # exclusions are asserted against this file by
  # `crates/itaruby_semantic/tests/stdlib_singletons.rs`, so a
  # regeneration that loses them fails the build instead of quietly
  # widening the file.
  #
  # Regenerate with:
  #
  #   ruby --disable-gems scripts/gen-stdlib-singleton-inventory.rb > crates/itaruby_semantic/declarations/stdlib_singletons.txt
  #
  # Generated with Ruby #{RUBY_VERSION} (#{RUBY_PLATFORM}).
HEADER

puts results.values.flatten.uniq.sort
