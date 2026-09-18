class NarrowOutsideFoo
  def bar
    1
  end
end

class NarrowOutsideWidget
  def check(x)
    if x.is_a?(NarrowOutsideFoo)
      x.bar
    end
    # Outside the refined branch (no early return, no else): `x` must
    # revert to its pre-if type (Unknown, `x` has no declared type), not
    # stay narrowed to `NarrowOutsideFoo`.
    x.nonexistent_method
  end
end
