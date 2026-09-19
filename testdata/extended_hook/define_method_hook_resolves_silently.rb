# Bead H, onda 2 — `base.define_method(:name) { ... }` in a
# `self.extended(base)` hook installs an instance method of the extender,
# through the same `define_method` every `def base.x`-style hook uses.
#
# MRI: exits 0 and prints "hi from the hook".
# `ita check`: silent.
module ExtHookDefineMethod
  def self.extended(base)
    base.define_method(:greet) { "hi from the hook" }
  end
end

class ExtHookDefineMethodUser
  extend ExtHookDefineMethod
end

puts ExtHookDefineMethodUser.new.greet