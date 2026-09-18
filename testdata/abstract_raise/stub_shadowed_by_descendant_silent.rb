# The discourse shape (endpoints/base.rb:280): a base method whose body
# contains `raise NotImplementedError` is an abstract stub, and EVERY
# descendant overrides it with a different signature. The call in the
# base's own body dispatches on the runtime (descendant) instance, so the
# stub's arity is never the one that runs — checking it would be a false
# E0102. Expected: zero diagnostics.
class AbstractStubShadowedBase
  def prepare(a, b)
    raise NotImplementedError
  end

  def run
    prepare(1, 2, 3)
  end
end

class AbstractStubShadowedKid < AbstractStubShadowedBase
  def prepare(a, b, c)
    a + b + c
  end
end

AbstractStubShadowedKid.new.run
