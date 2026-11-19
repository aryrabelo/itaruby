# Bead ita-qst: the cast comment binds to the ONE argument sharing its
# physical line only — a second argument on the NEXT line of the same
# multi-line call never inherits the previous line's cast. Since MATRIX
# H2 the `#:` sig judges only literal arguments, and a literal behind a
# cast is never judged: the uncast `:second` literal is the one E0103,
# and the call passing locals is silent.
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
  InlineCastSinkE.new.take_two(
    :first, #: as InlineCastFooE
    :second
  )
end
