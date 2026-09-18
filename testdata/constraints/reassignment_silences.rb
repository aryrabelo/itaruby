# Scenario (e): the binding is reassigned (to another Unknown value, via
# `yield`, so the second call still lands on the Ty::Unknown arm) between
# the two calls — the first call's constraint is discarded, leaving only
# one distinct-method constraint after the write, below the >= 2 dedupe
# threshold — silence.
def constr_reassign_silent(x)
  x.upcase
  x = yield
  x.push(1)
end
