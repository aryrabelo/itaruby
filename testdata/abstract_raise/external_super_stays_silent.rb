# Refinement (b): the abstract base's superclass is a DeclaredExternal
# constant (declared by the curated gems file, body never seen by this
# project), so the family's surface is provably incomplete and every
# lookup on it stays Inconclusive — `DeclaredExternal` reads as
# still-unknown. Expected: zero diagnostics (no E0101, and no E0104 —
# the declared name resolves).
class AbstractRaiseExtBase < ActiveRecord::Base
  def render
    raise NotImplementedError, "subclass"
  end

  def draw
    request
  end
end

class AbstractRaiseExtKid < AbstractRaiseExtBase
end

AbstractRaiseExtKid.new.draw
