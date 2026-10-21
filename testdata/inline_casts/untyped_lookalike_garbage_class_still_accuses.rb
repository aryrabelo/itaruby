# Bead ita-j0z control: `as UntypedLookalike` (a bare word that merely
# LOOKS like it could mean "erase the type" but is spelled as a
# capitalized, unresolvable project-class name, never the literal lower-
# case `untyped` token) must keep going through the existing `Named`
# arm — i.e. `resolve_ret_ty` fails to resolve it, and the value's real
# type is left untouched (`kept` hovers as the wrong class, never as
# untyped). Pins that adding the new `CastTarget::Untyped` variant did not
# accidentally widen the `untyped` match to any name containing that
# substring. Since MATRIX H2 the `#:` sig judges only literal arguments:
# the cast local is silent, the written `:wrong` accuses.
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
  kept = wrong #: as UntypedLookalike
  InlineCastUntypedLookalikeSink.new.take(
    kept, #: as UntypedLookalike
  )
  InlineCastUntypedLookalikeSink.new.take(:wrong)
end
