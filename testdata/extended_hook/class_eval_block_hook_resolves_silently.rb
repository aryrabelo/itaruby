# Bead H, onda 2 — `base.class_eval do ... end` with a LITERAL block: the
# block runs with `self` = base, so a bare `def` inside it is an instance
# method of the extender.
#
# MRI: exits 0 and prints "coded".
# `ita check`: silent.
module ExtHookClassEvalBlock
  def self.extended(base)
    base.class_eval do
      def coded
        "coded"
      end
    end
  end
end

class ExtHookClassEvalBlockUser
  extend ExtHookClassEvalBlock
end

puts ExtHookClassEvalBlockUser.new.coded