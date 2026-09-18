# Bead ita-j0z control: `as UntypedLookalike` (a bare word that merely
# LOOKS like it could mean "erase the type" but is spelled as a
# capitalized, unresolvable project-class name, never the literal lower-
# case `untyped` token) must keep going through the existing `Named`
# arm — i.e. `resolve_ret_ty` fails to resolve it, and the argument's
# real (mismatched) type is left untouched. Pins that adding the new
# `CastTarget::Untyped` variant did not accidentally widen the `untyped`
# match to any name containing that substring.
class InlineCastUntypedLookalikeWrong
end

class InlineCastUntypedLookalikeRight
end

class InlineCastUntypedLookalikeSink
  #: (InlineCastUntypedLookalikeRight) -> void
  def take(value)
  end
end

def inline_cast_untyped_lookalike_still_accuses
  wrong = InlineCastUntypedLookalikeWrong.new
  InlineCastUntypedLookalikeSink.new.take(
    wrong, #: as UntypedLookalike
  )
end
