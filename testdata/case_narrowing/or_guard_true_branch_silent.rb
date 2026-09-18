class CaseNarOrTrueBranchFoo
  def bar
    1
  end
end

class CaseNarOrTrueBranchWidget
  def check(cond)
    x = cond ? CaseNarOrTrueBranchFoo.new : nil

    if x.nil? || rand > 0.5
      # `x.nil? || rand > 0.5` can be true purely because of `rand > 0.5`,
      # with `x` not nil at all — narrowing `x` to `Nil` in this branch
      # would risk a false E0101 on a real method. Must stay silent (a
      # no-op here, same as "no fact" for the true branch).
      x.bar
    end
  end
end
