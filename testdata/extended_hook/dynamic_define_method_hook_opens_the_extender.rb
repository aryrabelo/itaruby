# Bead H, onda 2 — fail-closed, shape 4: `base.define_method(name)` with
# the name in a VARIABLE. The method really is installed; which name it
# is, no AST here can say, so the extender's surface is open.
#
# MRI: exits 0 and prints "named at runtime".
# `ita check`: silent.
module ExtHookDynamicDefineMethod
  def self.extended(base)
    name = :named_at_runtime
    base.define_method(name) { "named at runtime" }
  end
end

class ExtHookDynamicDefineMethodUser
  extend ExtHookDynamicDefineMethod
end

puts ExtHookDynamicDefineMethodUser.new.named_at_runtime