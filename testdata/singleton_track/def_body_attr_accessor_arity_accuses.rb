# frozen_string_literal: true

# What the banked knowledge makes observable: a literal `attr_accessor`
# inside `def self.install` defines a zero-argument reader, and MRI
# raises `ArgumentError: wrong number of arguments (given 1, expected 0)`
# on line 12.
class Boot
  def self.install
    attr_accessor :mode
  end
end

Boot.install
Boot.new.mode(1)
