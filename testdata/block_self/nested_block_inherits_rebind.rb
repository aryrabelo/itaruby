# Bead ita-uye, NESTING. The inner `each` is proven lexical on its own, but
# it is nested inside a rebound DSL block, so the enclosing `self` is
# already unknowable — "lexical self" means the self of the enclosing scope,
# and that scope is the rebound one. `rebindable_block_depth` composes for
# free: the inner block simply does not raise the count that the outer block
# already raised. Must stay silent.
class BlkSelfNestedCtx
  def rows(&blk)
    instance_exec(&blk)
  end

  def blkself_nested_context_only
    [1, 2]
  end
end

class BlkSelfNested
  def build
    ctx = BlkSelfNestedCtx.new
    ctx.rows { 2.times { blkself_nested_context_only } }
  end
end
