# Defect A (bead ita-exc): `DefWalker::walk_stmt`'s `CallNode` arm only
# specially walks `define_method` with a literal symbol and recognized
# `sig{}` blocks; every other class-body call with a block marks the class
# `open` and returns WITHOUT recursing into the block body, so a constant
# assigned inside it is never recorded in the enclosing class's `consts`.
# `constvis_enums` is an invented DSL method name on purpose: the fix must
# recurse into ANY class-body call block, not pattern-match a known name
# like `enums`/`sig`. Suppressing this is correct Ruby: constant
# *definition* is lexical — `instance_eval`/`instance_exec` rebind `self`
# but never the cref, so `ConstVisAlpha = "A"` inside the block still
# defines the constant on `ConstVisEnum`. Must become silent (bare
# self-reference, same file).
class ConstVisEnum
  constvis_enums do
    ConstVisAlpha = "A"
  end

  def describe
    case self
    when ConstVisAlpha then "alpha"
    end
  end
end
