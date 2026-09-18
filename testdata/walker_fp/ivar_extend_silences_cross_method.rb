module WalkerFpIvarExtendHelper
  def walker_fp_extended_method
    42
  end
end

class WalkerFpIvarExtendBasic
end

# Bead ita-o8l.4: `.extend(Module)` on an ivar receiver, called in one
# method (`setup`), read in a DIFFERENT method (`test_it`) — the exact
# shape of the real corpus site (actionview's `AssetUrlHelperControllerTest`,
# `@controller.extend ActionView::Helpers::AssetUrlHelper` in `setup`,
# `@controller.asset_path` in `test_asset_path`). Widening must survive
# across method boundaries to silence this.
class WalkerFpIvarExtendTest
  def setup
    @target = WalkerFpIvarExtendBasic.new
    @target.extend WalkerFpIvarExtendHelper
  end

  def test_it
    @target.walker_fp_extended_method
  end
end
