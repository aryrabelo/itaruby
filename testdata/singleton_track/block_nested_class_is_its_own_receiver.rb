# frozen_string_literal: true

# Bead ita-blk: a `class` KEYWORD inside a class-body block takes its cref
# from the LEXICAL scope, so this really defines `BlkHost::BlkFoo` — a
# different class from the top-level `BlkFoo` above. Until the walker
# registered it, `BlkFoo.blk_config` inside the block resolved to that
# unrelated top-level stub and read as a conclusive miss (4 of rails' 33
# residue records, `railties/test/railties/railtie_test.rb`).
class BlkFoo; end

class BlkRailtie
  def self.blk_config
    :cfg
  end
end

class BlkHost
  def self.blk_test(_name)
    yield
  end

  blk_test "config is available to the railtie" do
    class BlkFoo < BlkRailtie
    end

    raise "bad" unless BlkFoo.blk_config == :cfg
  end
end

raise "bad" unless BlkHost::BlkFoo != BlkFoo
