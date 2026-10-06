# Issue #6, FP-B: a gem's top-level module (forem reopens
# `honeycomb-beeline`'s `Honeycomb` this way) that the project writes only
# to nest its own class under it. The gem defines `add_field`; this tree
# writes nothing on the module, so both calls run under MRI. They must stay
# silent and bucket `open(Project(NamespaceOnlyModule))`. CO-S removes the
# pass and must emit E0101 at 10:15 and 15:9.
module Beeline
  class NoiseSampler
    def sample(rate)
      Beeline.add_field(:sample_rate, rate)
    end
  end
end

Beeline.add_field(:plan, "pro")
