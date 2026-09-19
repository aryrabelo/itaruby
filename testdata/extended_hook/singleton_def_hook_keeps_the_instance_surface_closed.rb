# Bead H, onda 2 — `def base.x` inside a `self.extended(base)` hook
# installs on the base OBJECT's SINGLETON, never on its instances. Filing
# that name as an instance method would silence a real `NoMethodError` on
# `User.new.pi`, which is the control this fixture is: the install is
# real, and it is one surface away from the call.
#
# MRI: prints 3.14 for `ExtHookSingletonUser.pi`, then raises NoMethodError
# on `ExtHookSingletonUser.new.pi`. `ita check`: exactly ONE error, on the
# instance call.
module ExtHookSingleton
  def self.extended(base)
    def base.pi
      3.14
    end
  end
end

class ExtHookSingletonUser
  extend ExtHookSingleton
end

puts ExtHookSingletonUser.pi

ExtHookSingletonUser.new.pi