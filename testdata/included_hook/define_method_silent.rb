# Bead ita-1yw silence fixture: `base.define_method(:greet)` in the hook
# defines `greet` on every includer — calling it must NOT diagnose (E0101).
module IncHookDefs
  def self.included(base)
    base.define_method(:greet) { "hi" }
  end
end

class IncHookDefUser
  include IncHookDefs

  def go
    greet
  end
end
