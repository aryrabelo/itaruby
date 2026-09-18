# Control: a class with NO own `self.new` keeps today's exact behavior
# — arity comes from `initialize`, unaffected by this bead.
class NewSelfOvNoOverrideWrongArity
  def initialize(a)
  end
end

NewSelfOvNoOverrideWrongArity.new
