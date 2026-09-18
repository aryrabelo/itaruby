class TestFwMixinMinitestTarget
  def real_method
    1
  end
end

class TestFwMixinMinitestSpec
  def check
    target = TestFwMixinMinitestTarget.new
    # `require "minitest/mock"` monkeypatches `Object#stub` — the CALL
    # to `.stub` itself is what Round-5 measured as FP (4 rails sites,
    # e.g. actionpack/test/dispatch/debug_exceptions_test.rb:586
    # `backtrace_cleaner.stub :clean, [...] do ... end`). The method
    # named INSIDE the block (`real_method` here) is a real method on
    # TestFwMixinMinitestTarget regardless, so this fixture only pins
    # the `.stub` call site, never the interprocedural "temporarily
    # redefined method" case (out of scope, bead ita-se5).
    target.stub(:real_method, 99) do
      target.real_method
    end
  end
end
