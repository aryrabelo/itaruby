class NarrowEarlyReturnFoo
  def bar
    1
  end
end

class NarrowEarlyReturnWidget
  def check_return(cond)
    x = cond ? NarrowEarlyReturnFoo.new : nil
    return if x.nil?
    # Past the early return, `x` can no longer be nil — narrowed to
    # `NarrowEarlyReturnFoo`, so an unknown method here must be caught.
    x.nonexistent_method
  end

  def check_raise(cond)
    x = cond ? NarrowEarlyReturnFoo.new : nil
    raise "x is nil" if x.nil?
    x.nonexistent_method
  end
end
