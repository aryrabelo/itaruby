# Bead H, onda 2 — fail-closed, shape 2: `base.instance_eval { ... }`
# runs its block with `self` = base, and a `define_method` inside it lands
# on the base's INSTANCE surface (this is the same double indirection
# ActiveSupport's own `execute_hook` relies on). The call itself names
# nothing, so the surface is open.
#
# MRI: exits 0 and prints "hidden".
# `ita check`: silent.
module ExtHookInstanceEval
  def self.extended(base)
    base.instance_eval do
      define_method(:hidden) { "hidden" }
    end
  end
end

class ExtHookInstanceEvalUser
  extend ExtHookInstanceEval
end

puts ExtHookInstanceEvalUser.new.hidden