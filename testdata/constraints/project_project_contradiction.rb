# Scenario (b): two disjoint, closed project classes each own-defining one
# of the two called methods — FIRES E0107.
class ConstrAlpha
  def alpha_only
  end
end

class ConstrBeta
  def beta_only
  end
end

def constr_project_conflict(x)
  x.alpha_only
  x.beta_only
end
