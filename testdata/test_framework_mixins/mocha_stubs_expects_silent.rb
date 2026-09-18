class TestFwMixinMochaTarget
  def real_method
    1
  end
end

class TestFwMixinMochaSpec
  def check
    obj = TestFwMixinMochaTarget.new
    # Mocha (`mocha/minitest`) mixes `stubs`/`expects`/`unstub` into
    # Object at require-time — none of the three exist on
    # TestFwMixinMochaTarget itself, so a checker with no knowledge of
    # the gem sees a closed project class and a name nowhere in its
    # ancestry: the exact false-E0101 shape measured in Round-5 (11
    # discourse sites, e.g. spec/lib/onebox/matcher_spec.rb:61).
    obj.stubs(:real_method).returns(42)
    obj.expects(:real_method)
    obj.unstub(:real_method)
  end
end
