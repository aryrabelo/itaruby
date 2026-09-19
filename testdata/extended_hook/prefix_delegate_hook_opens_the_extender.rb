# Bead H, onda 2 — fail-closed, shape 6: a `delegate` keyword that REWRITES
# the installed name. `delegate :name, to: :class, prefix: true` installs
# `class_name`, so the positional symbols are NOT the set that lands on the
# extender; nothing this AST reads names the real method, and the surface
# is open.
#
# MRI: exits 0 and prints "ExtHookPrefixDelegateUser" — the stand-in below
# implements `prefix:`'s name rewriting, so the line under test really does
# go through the spelling whose name the checker cannot derive.
# `ita check`: silent.
class Module
  def delegate(*names, to:, prefix: false, **)
    names.each do |n|
      installed = prefix ? :"#{to}_#{n}" : n
      define_method(installed) { |*| public_send(to) }
    end
  end
end

module ExtHookPrefixDelegate
  def self.extended(base)
    base.delegate :name, to: :class, prefix: true
  end
end

class ExtHookPrefixDelegateUser
  extend ExtHookPrefixDelegate
end

puts ExtHookPrefixDelegateUser.new.class_name