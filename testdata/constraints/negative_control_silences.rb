# Scenario (f, negative control): two closed project classes both
# own-define BOTH called methods — the intersection has 2 members
# (`UnionCandidate`, not empty) — silence. This is the mutation-proof
# fixture for the "always empty" direction: an intersection bug that
# always returns empty would wrongly fire E0107 here.
class ConstrSharedEpsilon
  def shared_alpha
  end

  def shared_beta
  end
end

class ConstrSharedZeta
  def shared_alpha
  end

  def shared_beta
  end
end

def constr_negative_silent(x)
  x.shared_alpha
  x.shared_beta
end
