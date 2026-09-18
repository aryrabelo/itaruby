# frozen_string_literal: true

# Inside `class << self` EVERY method-defining form lands on the class
# object: `define_method` in both its block and its argument spelling,
# `alias_method`, and the `alias` keyword — not just `def` and `attr_*`.
# MRI runs this file to completion.
class Config
  class << self
    define_method(:stats) { 1 }
    define_method(:stats4, proc { 5 })
    alias_method :stats2, :stats

    def other
      2
    end
    alias stats3 other
  end
end

raise 'stats' unless Config.stats == 1
raise 'stats4' unless Config.stats4 == 5
raise 'stats2' unless Config.stats2 == 1
raise 'other' unless Config.other == 2
raise 'stats3' unless Config.stats3 == 2
