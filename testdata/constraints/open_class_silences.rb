# Scenario (c): the only definer of `gamma_only` is `open` (has a
# `method_missing`), so `closed_candidates_for` excludes it — the call's
# candidate set is empty, so the "every method has >= 1 candidate" gate
# fails and the whole binding stays silent, even though the OTHER call
# (`.upcase`) has a perfectly good candidate.
class ConstrOpenGamma
  def method_missing(name, *args)
  end

  def gamma_only
  end
end

def constr_open_silent(x)
  x.gamma_only
  x.upcase
end
