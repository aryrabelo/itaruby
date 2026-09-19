# Bead H, onda 2 — the keying controls.
#
# Same hook module for one class only. Two ways to get this wrong are
# pinned here, and both are real `NoMethodError`s under MRI:
#
# * `ExtHookKeyedBare` extends a module that has NO `extended` hook — an
#   `extend` with nothing to harvest must not soften anything.
# * `ExtHookKeyedUntouched` never extends anything. The install reaches
#   the class that took the hook, and no other class in the file.
#
# MRI: prints the first line, then raises NoMethodError on the second.
# `ita check`: exactly TWO errors, one per accusing line.
class Module
  def delegate(*names, to:, **)
    names.each { |n| define_method(n) { |*| public_send(to) } }
  end
end

module ExtHookKeyedNaming
  def self.extended(base)
    base.delegate :model_name, to: :class
  end
end

module ExtHookKeyedNoHook
  def helper
    "no hook here"
  end
end

class ExtHookKeyedTaker
  extend ExtHookKeyedNaming
end

class ExtHookKeyedBare
  extend ExtHookKeyedNoHook
end

class ExtHookKeyedUntouched
end

puts ExtHookKeyedTaker.new.model_name

ExtHookKeyedBare.new.model_name
ExtHookKeyedUntouched.new.model_name