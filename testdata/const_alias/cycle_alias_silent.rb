# Bead ita-54k G2: an alias cycle (`X = Y; Y = X`) must never panic or
# loop. Under the shared tri-state constant walk a cycle is INCONCLUSIVE
# (`ConstResolution::Ambiguous`), so `ConstAliasCycleA::NOPE` is
# suppression-only — no E0104 and no fabricated type. Genuine misses
# (a name that plainly resolves nowhere, a non-literal RHS) still accuse;
# only real uncertainty is silenced (invariant #1).
ConstAliasCycleA = ConstAliasCycleB
ConstAliasCycleB = ConstAliasCycleA

class ConstAliasCycleReader
  def read
    ConstAliasCycleA::NOPE
  end
end
