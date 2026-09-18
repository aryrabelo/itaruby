# Bead ita-qst: a cast that resolves fine (to a REAL project class) but
# still doesn't match the callee's own declared param type must keep
# accusing — the cast retypes the argument, it does not blanket-silence
# the call.
class InlineCastFooD
end

class InlineCastOtherD
end

class InlineCastSinkD
  #: (InlineCastOtherD) -> void
  def use_other(target)
  end
end

def inline_cast_still_invalid_d
  value = InlineCastFooD.new
  InlineCastSinkD.new.use_other(
    value, #: as InlineCastFooD
  )
end
