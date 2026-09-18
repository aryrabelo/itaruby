# Bead ita-qst: the cast comment binds to the ONE argument sharing its
# physical line only — a second argument on the NEXT line of the same
# multi-line call must keep its real (mismatched) type, never inherit the
# previous line's cast.
class InlineCastFooE
end

class InlineCastWrongE
end

class InlineCastSinkE
  #: (InlineCastFooE, InlineCastFooE) -> void
  def take_two(first, second)
  end
end

def inline_cast_no_leak_e
  a = InlineCastWrongE.new
  b = InlineCastWrongE.new
  InlineCastSinkE.new.take_two(
    a, #: as InlineCastFooE
    b
  )
end
