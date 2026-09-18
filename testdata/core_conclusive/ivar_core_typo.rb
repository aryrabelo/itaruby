# ita-2ve closed-world, FIRES: `@tag = "..."` in initialize types the
# ivar as String (W2), `push` exists on Array but not on String, the
# inventory confirms the miss -> E0101 (the p3 A/B gap).
class CoreConclusiveLabel
  def initialize
    @tag = "hello"
  end

  def mutate
    @tag.push(1)
  end
end
