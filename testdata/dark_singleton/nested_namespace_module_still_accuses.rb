# The rule stops at the top level. A namespace-only module nested under the
# project's own namespace is that project's organization (discourse's
# `DiscourseAi::Utils::DiffUtils`, whose misses the public baseline holds as
# true positives), so a miss on it accuses. CO-T drops the top-level check
# and must silence 17:13.
module Sigil
  def self.root
    "."
  end

  module Util
    class Parser
    end
  end
end

Sigil::Util.parse("x")
