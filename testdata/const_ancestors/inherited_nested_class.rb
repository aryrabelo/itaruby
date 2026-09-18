# A nested class (not a `CONST = ...` assignment) reached through the
# superclass must resolve too — the ancestor walk checks nested paths in the
# index, not only constant assignments. Silent.
module ConstAncNest
  class ConstAncNestBase
    class ConstAncInner; end
  end

  class ConstAncNestChild < ConstAncNestBase
    def inner
      ConstAncInner
    end
  end
end
