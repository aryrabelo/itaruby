# Shape (3): `extend` of a non-constant. The module is chosen at runtime,
# so the class's singleton surface is unknowable and the class must be
# OPEN. Already handled by OpenReason::DynamicMixinArg before step N+1 -
# this fixture is the proof, not a new mechanism.
module Metric
  def sample
    "sample"
  end
end

class Probe
  chosen = Metric
  extend chosen

  def self.known(a)
    a
  end
end

raise "expected the runtime-chosen module" unless Probe.sample == "sample"
