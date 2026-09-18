# Bead ita-54k G2: an alias cycle (`X = Y; Y = X`) must degrade to a
# silent MISS in the alias-chase mechanism itself — never a panic, never
# an infinite loop. Neither name is ever a real constant anywhere in the
# project, so `ConstAliasCycleA::NOPE` stays a genuine, correctly
# reported E0104: a wrong suppression here would be just as much a bug
# as a crash (invariant #1 forbids fabricating a hit, not just fabricating
# a diagnostic).
ConstAliasCycleA = ConstAliasCycleB
ConstAliasCycleB = ConstAliasCycleA

class ConstAliasCycleReader
  def read
    ConstAliasCycleA::NOPE
  end
end
