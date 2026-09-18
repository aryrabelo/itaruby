# A descendant's MIXIN can supply the hook: `ArMixinKid` includes a plain
# closed module defining `request`, so the base's self-send resolves at
# runtime through the descendant's ancestry. The family walk reads each
# member's full chain (includes included) for the name, so this must stay
# silent — the control that keeps the name-keyed walk from inventing an
# E0101.
module AbstractRaiseMixinProvider
  def request
    "ok"
  end
end

class AbstractRaiseMixinBase
  def render
    raise NotImplementedError, "subclass"
  end

  def draw
    request
  end
end

class AbstractRaiseMixinKid < AbstractRaiseMixinBase
  include AbstractRaiseMixinProvider
end

AbstractRaiseMixinKid.new.draw
