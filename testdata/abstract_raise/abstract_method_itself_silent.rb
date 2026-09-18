# The abstract method itself resolves: `render` IS defined — its body
# raises. A self-send of `render` from another method of the same
# abstract base is Found, never Inconclusive, never NotFound. Expected:
# zero diagnostics.
class AbstractRaiseSelfCallBase
  def render
    raise NotImplementedError, "subclass"
  end

  def paint
    render
  end
end

AbstractRaiseSelfCallBase.new.paint
