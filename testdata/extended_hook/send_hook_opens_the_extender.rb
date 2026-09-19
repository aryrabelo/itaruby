# Bead H, onda 2 — fail-closed, shape 1: the hook installs through
# `base.send(:define_method, :name)`, so the NAME SET is a runtime value
# this AST cannot read. The extender's instance surface is really
# changed, so the whole surface is open.
#
# MRI: exits 0 and prints "secret".
# `ita check`: silent.
module ExtHookSend
  def self.extended(base)
    base.send(:define_method, :secret) { "secret" }
  end
end

class ExtHookSendUser
  extend ExtHookSend
end

puts ExtHookSendUser.new.secret