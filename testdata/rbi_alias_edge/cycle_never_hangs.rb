# Bead ita-4wq mandatory cycle-guard proof: `RbiY4wqCycleOuter::A` and
# `::B` alias each other, both written inside the vendored RBI. This
# reference must degrade to a silent miss (genuine E0104), never hang —
# asserted with an explicit bounded-time run in the test file, not just
# by the whole test binary completing at all.
class RbiY4wqCycleReferencer
  def bad
    RbiY4wqCycleOuter::A::Whatever
  end
end
