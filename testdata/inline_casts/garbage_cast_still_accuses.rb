# Bead ita-qst: an unresolvable cast target (`Blah` names no project
# class) must never invent a type — the value keeps its real inferred
# type (`kept` hovers as `InlineCastWrongC`), byte-identical to having no
# cast comment at all. Since MATRIX H2 the `#:` sig judges only literal
# arguments: the cast local is silent, the written `:not_foo` accuses.
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
  kept = wrong #: as Blah
  InlineCastSinkC.new.use_foo(
    kept, #: as Blah
  )
  InlineCastSinkC.new.use_foo(:not_foo)
end
