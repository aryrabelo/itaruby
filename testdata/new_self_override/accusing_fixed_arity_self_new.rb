# Bead ita-6bq: a class that defines its OWN singleton `new` with a
# plain, fixed-arity signature routes `.new(...)`'s arity check through
# THAT signature, not through `initialize` — proven here by giving
# `initialize` an arity (2) that would silently MATCH the 2-argument
# call below if it were still the thing being checked. `self.new(x)`
# expects exactly 1 argument, so the 2-argument call must fire E0102.
class NewSelfOvFixed
  def self.new(x)
    allocate
  end

  def initialize(a, b)
  end
end

NewSelfOvFixed.new(1, 2)
