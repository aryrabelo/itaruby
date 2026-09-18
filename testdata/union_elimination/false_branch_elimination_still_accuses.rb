# Bead ita-zdy control: elimination must narrow to the CORRECT remaining
# member, not just any concrete type. `only_on_a` exists on
# `UnionElimAccuseA` and NOT on `UnionElimAccuseB`. In the false branch of
# `x.is_a?(UnionElimAccuseA)`, `x` must narrow to exactly
# `UnionElimAccuseB` — calling `only_on_a` there is genuinely invalid and
# MUST still accuse E0101 (invariant #1 is about false positives, never
# about swallowing real errors). A wrong implementation that removes the
# non-tested member instead (keeping `A`) would leave this call silent.
class UnionElimAccuseA
  def only_on_a
  end
end

class UnionElimAccuseB
end

class UnionElimAccuseWidget
  #: (UnionElimAccuseA | UnionElimAccuseB) -> void
  def handle(x)
    if x.is_a?(UnionElimAccuseA)
      x.only_on_a
    else
      x.only_on_a
    end
  end
end
