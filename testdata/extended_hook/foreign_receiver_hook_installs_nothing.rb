# Bead H, onda 2 — the attribution control: only a call whose receiver is
# a bare read of the hook's OWN parameter (`base`) describes the base.
# `other` is a local holding somebody else entirely, and a name installed
# on it must not appear on the extender.
#
# (A local, not a constant, on purpose: the guard under test is the NAME
# comparison against the hook's parameter, and any local read would pass a
# guard that only asked "is this a local?".)
#
# MRI: installs `ghost` on `Other`, then raises NoMethodError on the
# extender's call. `ita check`: exactly ONE error, on the extender's call.
module ExtHookForeignReceiver
  class Other
    def self.delegate(*names, to:)
      names.each { |n| define_method(n) { |*| self.class } }
    end
  end

  def self.extended(base)
    other = Other
    other.delegate :ghost, to: :class
    base.instance_variable_set(:@took_the_hook, true)
  end
end

class ExtHookForeignReceiverUser
  extend ExtHookForeignReceiver
end

puts ExtHookForeignReceiver::Other.new.ghost

ExtHookForeignReceiverUser.new.ghost