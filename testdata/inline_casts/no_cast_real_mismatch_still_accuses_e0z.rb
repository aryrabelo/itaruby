# Bead ita-j0z control: a genuine type mismatch with NO cast comment at
# all must still accuse — proves the new `Untyped` variant didn't
# accidentally widen silence beyond lines that actually carry
# `#: as untyped`.
class InlineCastUntypedControlWrong
end

class InlineCastUntypedControlRight
end

class InlineCastUntypedControlSink
  #: (InlineCastUntypedControlRight) -> void
  def take(value)
  end
end

def inline_cast_untyped_control_still_accuses
  wrong = InlineCastUntypedControlWrong.new
  InlineCastUntypedControlSink.new.take(wrong)
end
