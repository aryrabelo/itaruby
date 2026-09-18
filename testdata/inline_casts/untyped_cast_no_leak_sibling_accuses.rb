# Bead ita-j0z: the `#: as untyped` cast binds to the ONE argument
# sharing its physical line only — a sibling argument on a DIFFERENT
# line of the same multi-line call must keep its real (mismatched) type,
# never inherit the previous line's erasure. Mutant (b): if the cast
# were applied to the whole call/line instead of the one argument, this
# sibling would go silent instead of accusing.
class InlineCastUntypedNoLeakFoo
end

class InlineCastUntypedNoLeakWrong
end

class InlineCastUntypedNoLeakSink
  #: (InlineCastUntypedNoLeakFoo, InlineCastUntypedNoLeakFoo) -> void
  def take_two(first, second)
  end
end

def inline_cast_untyped_no_leak
  a = InlineCastUntypedNoLeakWrong.new
  b = InlineCastUntypedNoLeakWrong.new
  InlineCastUntypedNoLeakSink.new.take_two(
    a, #: as untyped
    b
  )
end
