class WalkerFpIvarNoExtendBasic
end

# Negative control for bead ita-o8l.4: no `.extend` anywhere on this ivar,
# so the widening must never fire and the genuine unknown-method call
# still accuses.
class WalkerFpIvarNoExtendTest
  def setup
    @target = WalkerFpIvarNoExtendBasic.new
  end

  def test_it
    @target.walker_fp_never_defined_anywhere
  end
end
