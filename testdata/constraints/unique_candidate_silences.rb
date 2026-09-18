# Scenario (d): both methods are only ever defined by one closed project
# class — the intersection is exactly that one class (`Inferred`, not a
# contradiction) — silence.
class ConstrUniqueDelta
  def delta_alpha
  end

  def delta_beta
  end
end

def constr_unique_ok(x)
  x.delta_alpha
  x.delta_beta
end
