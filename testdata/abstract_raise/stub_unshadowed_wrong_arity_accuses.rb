# The other side of the stub rule: with NO descendant overriding the
# name, the stub itself is the effective method — a wrong-arity call to
# it is a real latent ArgumentError and must still accuse. Expected:
# exactly one E0102 at the 3-arg call (11:5).
class AbstractStubUnshadowedBase
  def prepare(a, b)
    raise NotImplementedError
  end

  def run
    prepare(1, 2, 3)
  end
end

class AbstractStubUnshadowedKid < AbstractStubUnshadowedBase
  def paint
    1
  end
end

AbstractStubUnshadowedKid.new.run
