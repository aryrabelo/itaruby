# Control for G2: a class with its OWN `initialize` (found, not
# NotFound) keeps full arity checking — the bead's fix only silences
# the NotFound verdict, never a real, modeled `initialize`.
class NoInitZeroArityControl
  def initialize
  end
end

NoInitZeroArityControl.new(1)
