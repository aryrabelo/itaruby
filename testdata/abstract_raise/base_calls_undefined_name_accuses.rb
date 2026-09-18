# The abstract-raise bug (rails dd1c8848^, actionview Tags::SearchField):
# a base whose `render` carries `raise NotImplementedError` is the
# template-method idiom, NOT a licence for every lookup on the family to
# go Inconclusive. The base's own `draw` self-sends `request`, which no
# member of the family defines — a NAME the family never defines anywhere
# stays a real bug.
# BUG (silent before the fix): exactly one E0101 at the `request` call in
# `AbstractRaiseAccuseBase#draw` (15:5); zero everywhere else.
class AbstractRaiseAccuseBase
  def render
    raise NotImplementedError, "#{self.class} must implement render"
  end

  def draw
    request
  end
end

class AbstractRaiseAccuseKidOne < AbstractRaiseAccuseBase
  def paint
    1
  end
end

class AbstractRaiseAccuseKidTwo < AbstractRaiseAccuseBase
  def paint
    2
  end
end

AbstractRaiseAccuseKidOne.new.draw
