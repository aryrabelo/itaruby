# Bead ita-qst: `#: as Type` resolves a project class name and retypes the
# argument for THIS call's arg-type check — a genuinely mismatched arg
# becomes compatible once cast to the type it actually is at runtime.
class InlineCastFooB
end

class InlineCastWrongB
end

class InlineCastSinkB
  #: (InlineCastFooB) -> void
  def use_foo(target)
  end
end

def inline_cast_class_ok_b
  wrong = InlineCastWrongB.new
  InlineCastSinkB.new.use_foo(
    wrong, #: as InlineCastFooB
  )
end
