# Control, other arity direction: `initialize(x)` (arity 1) called with
# zero arguments must still raise E0102 — a real, modeled `initialize`
# is untouched by the bead's silence-on-NotFound fix.
class NoInitOneArityControl
  def initialize(x)
  end
end

NoInitOneArityControl.new
