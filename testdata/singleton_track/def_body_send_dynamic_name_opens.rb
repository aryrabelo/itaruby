# frozen_string_literal: true

# The other half of the unwrap: once `send(:define_method, ...)` is read
# as a definition, a DYNAMIC name under it is as unknowable as the bare
# `define_method(key)` is — the class must open, not stay closed with a
# surface it cannot enumerate. MRI runs this file to completion.
class Boot
  def self.install(key)
    send(:define_method, key) { 1 }
  end
end

Boot.install(:alpha)
raise 'alpha' unless Boot.new.alpha == 1
