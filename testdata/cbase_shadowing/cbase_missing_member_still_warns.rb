# Two-sided control for ita-yh1 (anti-suppression, mutant b): a cbase
# reference to a REAL top-level module but a member that does not exist on
# it must still raise E0104. The fix resolves a leading `::` to the top
# level; it must NOT become "any cbase path is treated as resolved" — that
# would silently drop this diagnostic (invariant #1: only `Ty::Unknown`
# stays silent, and only when nothing DISPROVES existence).
module CbaseShTarget2
  VALUE = "top-level"
end

module CbaseShHost2
  module CbaseShTarget2
  end

  class Consumer
    X = ::CbaseShTarget2::DOES_NOT_EXIST
  end
end
