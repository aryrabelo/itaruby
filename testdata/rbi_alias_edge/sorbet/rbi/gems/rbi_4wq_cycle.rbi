# typed: true
# DO NOT EDIT MANUALLY — bead ita-4wq's mandatory cycle-guard proof: a
# pathological alias pair, BOTH written inside a vendored RBI (never
# project code — the project-side `ProjectIndex::CONST_ALIAS_CHAIN_CAP`
# guard is a completely separate mechanism and does not cover this
# path), must never hang the checker.

module RbiY4wqCycleOuter
end

RbiY4wqCycleOuter::A = RbiY4wqCycleOuter::B
RbiY4wqCycleOuter::B = RbiY4wqCycleOuter::A
