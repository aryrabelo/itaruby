# frozen_string_literal: true

# The instance side of the same fact: a name `define_method` installs
# inside `class << self` is NOT an instance method, so this call is a
# certain `NoMethodError` — MRI raises on line 15.
class Config
  class << self
    define_method(:stats) { 1 }
    alias_method :stats2, :stats
  end
end

raise 'stats' unless Config.stats == 1

Config.new.stats
