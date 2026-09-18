# Refinement (a): a descendant open for ANY other reason — here
# `method_missing` — could define anything, so the whole family stays
# Inconclusive even though no descendant literally defines `request`.
# Expected: zero diagnostics.
class AbstractRaiseOpenBase
  def render
    raise NotImplementedError, "subclass"
  end

  def draw
    request
  end
end

class AbstractRaiseOpenMagicKid < AbstractRaiseOpenBase
  def method_missing(name, *args)
    nil
  end

  def paint
    1
  end
end

AbstractRaiseOpenMagicKid.new.draw
