class TestFwMixinControlTarget
end

class TestFwMixinControlSpec
  def check
    # Negative control: a name that is neither a real method, a core/
    # Kernel method, nor a test-framework mixin must keep accusing —
    # proves the new allowlist entries are narrow (exact names only),
    # not a blanket "any call in a Spec class is silent" softening.
    TestFwMixinControlTarget.new.totally_bogus_method
  end
end
