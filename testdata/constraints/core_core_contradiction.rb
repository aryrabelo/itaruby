# Scenario (a): two core-only constraint calls whose candidate sets are
# disjoint (`String#upcase` vs `Array#push`) — FIRES E0107. This is the
# planted fixture `scripts/gauntlet-gates.sh` gate c looks for.
def constr_core_conflict(x)
  x.upcase
  x.push(1)
end
