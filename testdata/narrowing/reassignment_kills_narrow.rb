class NarrowReassignFoo
  def bar
    1
  end
end

class NarrowReassignWidget
  def check(x, y)
    if x.is_a?(NarrowReassignFoo)
      # Reassigning `x` inside the refined branch must kill the narrowing
      # immediately — `y` is an untyped parameter (Unknown), so after this
      # line `x` is Unknown too, and the call below must stay silent.
      x = y
      x.nonexistent_method
    end
  end
end
