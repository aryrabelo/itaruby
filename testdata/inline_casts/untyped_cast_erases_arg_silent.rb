# Bead ita-j0z: `#: as untyped` must ERASE the argument's type
# (`Ty::Unknown`), not fall through to the original inferred type the way
# `apply_cast_comment`'s `Named` arm degrades for an unresolvable class
# name. Passing a genuinely wrong-typed argument, cast `#: as untyped`,
# into a sink declaring a real class param must produce zero
# diagnostics — Unknown satisfies every sig by construction
# (invariant #1). Real site: ruby-lsp's `addon_test.rb:149`, a settings
# mixin value cast `as untyped` specifically to erase its type.
class InlineCastUntypedWrong
end

class InlineCastUntypedRight
end

class InlineCastUntypedSink
  #: (InlineCastUntypedRight) -> void
  def take(value)
  end
end

def inline_cast_untyped_silences
  wrong = InlineCastUntypedWrong.new
  InlineCastUntypedSink.new.take(
    wrong, #: as untyped
  )
end
