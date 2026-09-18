module Ita519Outer2
  class Ita519Widget2
    def sibling_only; end
  end
end

class Ita519Widget2
  def top_level_method; end
end

# Same compact-syntax shape as the sibling fixture, but the mirror-image
# proof: `sibling_only` exists ONLY on the sibling, never on the top-level
# class this reference must resolve to. If resolution ever regresses back
# to the sibling (the original bug), this call would go silent instead of
# accusing — a real E0101 must never be swallowed by the nesting fix.
class Ita519Outer2::UsesCompact2
  def call
    Ita519Widget2.new.sibling_only
  end
end
