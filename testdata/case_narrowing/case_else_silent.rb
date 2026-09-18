class CaseNarElseFoo
  def bar
    1
  end
end

class CaseNarElseWidget
  def check(x)
    case x
    when CaseNarElseFoo
      x.bar
    else
      # Failing every `when` says nothing about what `x` actually is —
      # the `else` branch must stay unrefined (Unknown here, `x` has no
      # declared type), never wrongly narrowed to some other class.
      x.nonexistent_method
    end
  end
end
