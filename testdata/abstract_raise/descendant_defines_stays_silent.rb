# The softening this mechanism exists for: one descendant of the
# abstract-raise base DOES define `request`, so the base's bare self-send
# is a template-method hook supplied at runtime. Expected: zero
# diagnostics.
class AbstractRaiseSilentBase
  def render
    raise NotImplementedError, "subclass"
  end

  def draw
    request
  end
end

class AbstractRaiseSilentGoodKid < AbstractRaiseSilentBase
  def request
    "ok"
  end
end

class AbstractRaiseSilentOtherKid < AbstractRaiseSilentBase
  def paint
    2
  end
end

AbstractRaiseSilentGoodKid.new.draw
