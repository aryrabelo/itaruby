#!/usr/bin/env ruby
# W3 require/autoload: mechanical harvest of every constant path each
# stdlib `require` defines, straight from the Ruby runtime — the exact
# same pattern as gen-core-inventory.rb (bead ita-2ve). NOT a
# hand-maintained list: the lib list comes from RbConfig's own lib dirs,
# and every constant line comes from the language's reflection, which is
# the anti-gaming rule's generated-content exception (versioned generator
# + regen header).
#
#   ruby --disable-gems scripts/gen-stdlib-inventory.rb > \
#     crates/itaruby_semantic/declarations/stdlib_constants.txt
#
# One fresh subprocess per lib (`RbConfig.ruby --disable-gems`), so no
# require can pollute another's constant set. A subprocess that fails to
# require (LoadError, crash) contributes zero lines — never aborts the
# harvest.
#
# Guard: same as core_inventory — a Ruby older than the one this file was
# first generated with (3.4.2) would shrink the constant set and turn
# suppressed E0104s back on downstream. Refuse instead.
abort("need Ruby >= 3.0 (generated with 3.4.2); found #{RUBY_VERSION}") if RUBY_VERSION.to_f < 3.0

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

# The child does its own pre/post diff, so a shared-core constant
# (Object, Kernel, RUBY_VERSION, ...) is never attributed to a stdlib
# lib. Nested namespaces are walked recursively (own constants only,
# `constants(false)`), with a cycle guard; depth is unbounded in principle
# but stdlib namespaces are shallow — a hard cap of 8 documents the
# ceiling without silently truncating anything real (deepest stdlib
# namespace today is 3).
CHILD = <<~RB
  pre = Object.constants
  begin
    require ARGV[0]
  rescue Exception
    exit 2
  end
  def walk(mod, prefix, depth, seen, out)
    return if depth >= 8 || seen.include?(mod.object_id)
    seen << mod.object_id
    mod.constants(false).each do |c|
      path = prefix.empty? ? c.to_s : "\#{prefix}::\#{c}"
      out << path
      begin
        v = mod.const_get(c)
      rescue NameError
        next
      end
      walk(v, path, depth + 1, seen, out) if v.is_a?(Module)
    end
  end
  out = []
  (Object.constants - pre).each do |c|
    path = c.to_s
    out << path
    v = Object.const_get(c) rescue nil
    walk(v, path, 0, [], out) if v.is_a?(Module)
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

puts <<~HEADER
  # itaruby stdlib constant inventory (W3 require/autoload) — GENERATED
  # FILE, do not hand-edit.
  #
  # One `<lib>\t<Constant::Path>` line per constant path that requiring
  # stdlib lib `<lib>` defines (top-level or nested), mechanically
  # harvested from the Ruby runtime — never written by hand. Consumed by
  # `index.rs::stdlib_declares`: an unresolved constant reference whose
  # full path appears here AND whose defining lib the project actually
  # `require`s somewhere suppresses E0104 (declaration, open ancestry,
  # same contract as declarations/gems.rbi). Without the require in the
  # project, the constant keeps warning.
  #
  # Regenerate with:
  #
  #   ruby --disable-gems scripts/gen-stdlib-inventory.rb > crates/itaruby_semantic/declarations/stdlib_constants.txt
  #
  # Generated with Ruby #{RUBY_VERSION} (#{RUBY_PLATFORM}).
HEADER

results.sort.each do |lib, paths|
  paths.uniq.sort.each do |path|
    puts "#{lib}\t#{path}"
  end
end
