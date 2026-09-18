# Bead ita-qst: an unresolvable cast target (`Blah` names no project
# class) must never invent a type — the argument keeps its real inferred
# type and the genuine mismatch still fires, byte-identical to having no
# cast comment at all.
class InlineCastFooC
end

class InlineCastWrongC
end

class InlineCastSinkC
  #: (InlineCastFooC) -> void
  def use_foo(target)
  end
end

def inline_cast_garbage_c
  wrong = InlineCastWrongC.new
  InlineCastSinkC.new.use_foo(
    wrong, #: as Blah
  )
end
