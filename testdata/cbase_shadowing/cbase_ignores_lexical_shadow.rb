# Bead ita-yh1: `::CbaseShTarget::VALUE` is a cbase (`::`-prefixed)
# constant path. Real Ruby cbase semantics: resolution of a `::`-prefixed
# head starts at the TOP LEVEL ONLY, never through lexical nesting.
# `CbaseShHost::CbaseShTarget` here is an unrelated module that happens to
# share the simple name `CbaseShTarget` and lexically SHADOWS it — a naive
# lexical resolve of a path with the leading `::` stripped would find that
# shadow first, fail to find `VALUE` on it, and falsely emit E0104. The
# real target is the top-level `::CbaseShTarget`, reachable only by
# honoring the leading `::` all the way through both the typing path
# (`resolve_const_through_aliases`) and the diagnostic path
# (`check_const_ref`/`const_exists`).
module CbaseShTarget
  VALUE = "top-level"
end

module CbaseShHost
  module CbaseShTarget
    # Deliberately empty: this shadow must never be consulted for a cbase
    # reference — its presence is exactly what used to break resolution.
  end

  class Consumer
    X = ::CbaseShTarget::VALUE
  end
end
