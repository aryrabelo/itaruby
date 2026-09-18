class CaseNarEarlyReturnFoo
  def bar
    1
  end
end

class CaseNarEarlyReturnWidget
  def check(cond)
    x = cond ? CaseNarEarlyReturnFoo.new : nil
    return unless x
    # Past the early return, `x` can no longer be nil (nor false) —
    # narrowed to `CaseNarEarlyReturnFoo`, so an unknown method here must
    # be caught. `narrow_of` previously only recognized `x.nil?`/
    # `x.is_a?` calls, not this bare truthy predicate.
    x.nonexistent_method
  end
end
