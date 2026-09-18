class NarrowIsAFoo
  def bar
    1
  end
end

class NarrowIsAWidget
  def check(x)
    if x.is_a?(NarrowIsAFoo)
      x.nonexistent_method
    end
  end
end
