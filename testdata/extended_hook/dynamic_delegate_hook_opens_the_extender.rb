# Bead H, onda 2 — fail-closed, shape 5: `base.delegate(*names, to:
# :class)` — a splat, so the installed NAMES are a runtime value. The
# names really are installed, so the extender's surface is open.
#
# MRI: exits 0 and prints "generated".
# `ita check`: silent.
class Module
  def delegate(*names, to:, **)
    names.each { |n| define_method(n) { |*| public_send(to) } }
  end
end

module ExtHookDynamicDelegate
  def self.extended(base)
    names = [:generated_name]
    base.delegate(*names, to: :class)
  end
end

class ExtHookDynamicDelegateUser
  extend ExtHookDynamicDelegate
end

puts ExtHookDynamicDelegateUser.new.generated_name