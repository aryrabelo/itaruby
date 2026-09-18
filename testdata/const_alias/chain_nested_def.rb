# Bead ita-47y: alias chaining (`A = B; B = C`, `C` real) referenced with
# an extra segment past the FIRST hop — must resolve exactly as
# `ConstAlias47yChainTarget::Deep::VALUE` would. Bead ita-54k's own
# `resolve_const_via_alias` already chases the A->B->C chain itself; this
# fixture proves the chain's RESULT still feeds the new per-segment
# member walk (`resolve_const_through_aliases`), not just the bare name.
module ConstAlias47yChainTarget
  module Deep
    VALUE = 1
  end
end

ConstAlias47yChainA = ConstAlias47yChainB
ConstAlias47yChainB = ConstAlias47yChainTarget
