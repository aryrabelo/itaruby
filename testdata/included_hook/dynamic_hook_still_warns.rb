# Bead ita-1yw negative control: a hook using the NON-literal
# `base.class_eval(string)` form is out of scope — the attribute it defines
# must NOT be registered (fail-closed false negative, never a guess), so
# reading `secret` here keeps accusing E0101, proving the harvester never
# over-registers.
module IncHookDyn
  def self.included(base)
    base.class_eval("attr_accessor :secret")
  end
end

class IncHookDynUser
  include IncHookDyn

  def go
    secret
  end
end
