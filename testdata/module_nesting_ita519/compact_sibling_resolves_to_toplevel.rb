module Ita519Outer
  class Ita519Widget
    def sibling_only; end
  end
end

class Ita519Widget
  def top_level_method; end
end

# Compact syntax: Module.nesting inside this body is exactly
# [Ita519Outer::UsesCompact] (ONE level) — never [Ita519Outer,
# Ita519Outer::UsesCompact]. `Ita519Widget` below must resolve to the
# TOP-LEVEL class (which defines `top_level_method`), never to the sibling
# `Ita519Outer::Ita519Widget` (which only defines `sibling_only`).
class Ita519Outer::UsesCompact
  def call
    Ita519Widget.new.top_level_method
  end
end
