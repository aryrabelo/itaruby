# Bead ita-zdy: plain `if`/`is_a?` (not a ternary) on a closed
# project-class union. The false branch eliminates
# `UnionElimIfA`, leaving exactly `UnionElimIfB` — passed to a sink
# method that only accepts `UnionElimIfB`, a call that is valid ONLY on
# the remaining member. Without the elimination, `x` stays
# `UnionElimIfA | UnionElimIfB` there, and `compatible` rejects the `A`
# half against a `B`-only param: a real false positive.
class UnionElimIfA
end

class UnionElimIfB
end

class UnionElimIfSink
  #: (UnionElimIfB) -> void
  def take_b(only_b)
  end
end

class UnionElimIfWidget
  #: (UnionElimIfA | UnionElimIfB) -> void
  def handle(x)
    sink = UnionElimIfSink.new
    if x.is_a?(UnionElimIfA)
      # true branch: x narrows to UnionElimIfA, not exercised here.
    else
      sink.take_b(x)
    end
  end
end
