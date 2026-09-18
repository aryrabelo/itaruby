class NarrowElseFoo
  def bar
    1
  end
end

class NarrowElseWidget
  def check(x)
    if x.is_a?(NarrowElseFoo)
      x.bar
    else
      # `x` failing `is_a?(NarrowElseFoo)` says nothing about what it
      # actually is — the else branch must stay unrefined (Unknown here,
      # since `x` has no declared type), never wrongly narrowed to some
      # other class.
      x.nonexistent_method
    end
  end
end
