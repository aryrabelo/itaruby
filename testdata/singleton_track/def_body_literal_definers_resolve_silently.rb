# frozen_string_literal: true

# In a `def self.x` body `self` IS the class, so a literal definer there
# names exactly what it defines: `define_singleton_method` writes the
# class-object track, `define_method`/`attr_*`/`alias_method` write the
# instance track. MRI runs this file to completion.
class Boot
  def self.install
    define_singleton_method(:ready?) { true }
    define_method(:tick) { 1 }
    attr_accessor :mode
    alias_method :tock, :tick
  end
end

Boot.install

raise 'ready?' unless Boot.ready?

b = Boot.new
raise 'tick' unless b.tick == 1
raise 'tock' unless b.tock == 1
b.mode = :fast
raise 'mode' unless b.mode == :fast
