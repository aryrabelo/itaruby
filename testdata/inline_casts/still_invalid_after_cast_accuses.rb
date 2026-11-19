# Bead ita-qst: a cast that resolves fine (to a REAL project class)
# retypes the value to it (`cast` hovers as `InlineCastFooD`); it does not
# blanket-silence the callee. Since MATRIX H2 the `#:` sig judges only
# literal arguments: the cast local is silent, the written `:not_other`
# still accuses.
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
  cast = value #: as InlineCastFooD
  InlineCastSinkD.new.use_other(
    cast, #: as InlineCastFooD
  )
  InlineCastSinkD.new.use_other(:not_other)
end
