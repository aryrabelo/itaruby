# Bead H, onda 2 — fail-closed, shape 3: `base.class_eval("...")` with a
# STRING body. The string really is evaluated in base's scope, and no AST
# here can read the names it defines — the exact shape the `included`-hook
# family documents as unreadable.
#
# MRI: exits 0 and prints "coded".
# `ita check`: silent.
module ExtHookStringEval
  def self.extended(base)
    base.class_eval("def coded; 'coded'; end")
  end
end

class ExtHookStringEvalUser
  extend ExtHookStringEval
end

puts ExtHookStringEvalUser.new.coded