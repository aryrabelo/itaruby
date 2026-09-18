# Control for rescue_system_stack_error_silent.rb: same shape, but the
# rescued name is not a real Ruby constant (core, stdlib, or project) —
# must still accuse E0104. Proves the fix is additive, not a blanket
# softening of the rescue-clause constant check.
class CoreConstsRescuerControl
  def recurse
    recurse
  rescue CoreConstsTotallyMadeUpError => e
    e.message
  end
end
