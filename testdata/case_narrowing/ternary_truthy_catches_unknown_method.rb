class CaseNarTernaryFoo
  def bar
    1
  end
end

class CaseNarTernaryWidget
  def check(cond)
    x = cond ? CaseNarTernaryFoo.new : nil
    # Ternary `x ? ... : ...` is an `IfNode` under the hood with a bare
    # variable predicate — the true branch narrows `x` to non-nil the
    # same way `if x` does.
    x ? x.nonexistent_method : nil
  end
end
