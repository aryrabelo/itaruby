class CaseNarOrGuardFoo
  def bar
    1
  end
end

class CaseNarOrGuardWidget
  def check(cond)
    x = cond ? CaseNarOrGuardFoo.new : nil
    return if x.nil? || rand > 0.5
    # The only way `x.nil? || rand > 0.5` is false is `x.nil?` itself
    # being false — so past this early return `x` is narrowed to
    # non-nil, and an unknown method here must be caught.
    x.nonexistent_method
  end
end
