# frozen_string_literal: true

# The unwrapped call is a real definition with a real arity: MRI raises
# `ArgumentError: wrong number of arguments (given 1, expected 0)` on
# line 13.
class Boot
  def self.install
    send(:attr_accessor, :mode)
  end
end

Boot.install
Boot.new.mode(1)
