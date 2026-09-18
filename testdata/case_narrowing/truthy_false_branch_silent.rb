class CaseNarTruthyFalseFoo
  def bar
    1
  end
end

class CaseNarTruthyFalseWidget
  def check(cond)
    x = cond ? CaseNarTruthyFalseFoo.new : nil

    if x
      x.bar
    else
      # `x` being falsy here could mean `nil` OR `false` — the false
      # branch of a bare truthy check must stay unrefined (a no-op),
      # never manufacture `Nil`. A buggy narrow to `Nil` here would make
      # `bar` (a real method on `CaseNarTruthyFalseFoo`, not on
      # `NilClass`) wrongly fire E0101.
      x.bar
    end
  end
end
