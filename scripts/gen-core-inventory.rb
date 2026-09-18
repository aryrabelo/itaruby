#!/usr/bin/env ruby
# Bead ita-2ve: mechanical harvest of core-class instance methods straight
# from the Ruby runtime, one `Class#method` line per public/protected
# instance method (ancestors included, private excluded — a private method
# can never be a receiver call). Bead ita-d2 added a second harvest: one
# `::Name` line per top-level constant (`Object.constants` — classes,
# modules, and value constants like `ARGV`/`RUBY_VERSION` alike; a nested
# path like `Float::INFINITY` needs no separate line because
# `is_known_core_constant`/`check_const_ref` only ever match the HEAD
# segment, and `Float` is already a harvested `::Name`). Run with no gems
# loaded so the inventory reflects the language, not the local machine's
# bundle:
#
#   ruby --disable-gems scripts/gen-core-inventory.rb > \
#     crates/itaruby_semantic/declarations/core_inventory.txt
#
# The output is versioned and embedded via include_str! (same pattern as
# declarations/gems.rbi). This is NOT a hand-maintained allowlist: every
# line comes from the language's own reflection, which is exactly the
# "mechanically generated from a language source with a versioned
# generator" exception the anti-gaming rule grants.
#
# Guard: a Ruby older than the one this inventory was first generated
# with (3.4.2) would SHRINK the file — missing methods/constants turn
# into false E0101/E0104s downstream (invariant #1). Refuse instead of
# shrinking.
abort("need Ruby >= 3.0 (generated with 3.4.2); found #{RUBY_VERSION}") if RUBY_VERSION.to_f < 3.0

# Second guard, ita-d2 mutant (b): the whole point of `--disable-gems` is
# that nothing outside the language proper gets harvested. RubyGems
# itself undefines `Gem` under that flag (verified:
# `ruby --disable-gems -e 'p defined?(Gem)'` => nil, plain `ruby -e
# 'p defined?(Gem)'` => "constant") — so if `Gem` is defined here, the
# flag was dropped and any "core" constant/method below could really be a
# gem's, corrupting invariant #1's closed-world guarantee downstream.
abort(
  "must run with 'ruby --disable-gems' — RubyGems is loaded (Gem is " \
  "defined), so a harvested name could come from a gem instead of the " \
  "language"
) if defined?(Gem)

# Snapshot BEFORE this script defines any of its own top-level names:
# `CLASSES` below is itself a top-level constant, so harvesting
# `Object.constants` any later would leak `::CLASSES` into the inventory
# as a false "core" entry (caught by hand during ita-d2's dry run — a
# `diff` against the pre-change file showed `::CLASSES` sitting alongside
# every genuine addition).
TOP_LEVEL_CONSTANTS = Object.constants.sort.freeze

CLASSES = %w[
  Integer Float String Symbol Array Hash
  NilClass TrueClass FalseClass Object
].freeze

puts <<~HEADER
  # itaruby core method/constant inventory (beads ita-2ve, ita-d2) —
  # GENERATED FILE, do not hand-edit.
  #
  # One `Class#method` line per public/protected instance method of each
  # core class core.rs models. Ancestors included, private excluded:
  # String's lines already cover Comparable/Object/Kernel exactly as Ruby
  # resolves them at runtime. The closed-world E0101 path (check.rs) fires
  # only when a method is absent from here AND from core.rs's allowlist.
  #
  # One `::Name` line per top-level constant (`Object.constants`), e.g.
  # `::SystemStackError`. `is_known_core_constant`/`core.rs` reads these
  # to suppress E0104 on real core constants the project index cannot
  # resolve (they are not user-defined, but they are real).
  #
  # Mechanically harvested from the Ruby runtime itself — never written by
  # hand; the generator is versioned at scripts/gen-core-inventory.rb (the
  # anti-gaming rule's generated-content exception). Regenerate with:
  #
  #   ruby --disable-gems scripts/gen-core-inventory.rb > crates/itaruby_semantic/declarations/core_inventory.txt
  #
  # Generated with Ruby #{RUBY_VERSION} (#{RUBY_PLATFORM}).
HEADER

CLASSES.each do |klass|
  Kernel.const_get(klass).instance_methods.sort.each do |meth|
    puts "#{klass}##{meth}"
  end
end

puts
puts "# top-level constants (Object.constants under --disable-gems)"
TOP_LEVEL_CONSTANTS.each do |const|
  puts "::#{const}"
end
