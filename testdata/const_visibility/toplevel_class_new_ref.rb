# Defect B: referencing the top-level `ConstVisSentinel` custom exception
# class from another file in the same project. Must become silent.
class ConstVisRaiser
  def raise_it
    raise ConstVisSentinel, "boom"
  end
end
