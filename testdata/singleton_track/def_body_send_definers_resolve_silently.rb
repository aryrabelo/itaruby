# frozen_string_literal: true

# `send(:define_method, ...)` is the same definition wearing a dispatch:
# the projects that write it are reaching past a private definer, and
# one level of unwrapping makes every rule above apply unchanged. MRI
# runs this file to completion.
class Boot
  def self.install
    send(:define_singleton_method, :ready?) { true }
    public_send(:define_method, :tick) { 1 }
    __send__(:attr_accessor, :mode)
    send(:alias_method, :tock, :tick)
  end
end

Boot.install

raise 'ready?' unless Boot.ready?

b = Boot.new
raise 'tick' unless b.tick == 1
raise 'tock' unless b.tock == 1
b.mode = :fast
raise 'mode' unless b.mode == :fast
