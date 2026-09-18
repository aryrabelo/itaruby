class CaseNarUnresolvableFoo
end

class CaseNarUnresolvableOther
end

class CaseNarUnresolvableWidget
  def check(cond)
    x = cond ? CaseNarUnresolvableFoo.new : CaseNarUnresolvableOther.new

    case x
    when CaseNarUnresolvableFoo, 1..10
      # One condition resolves to a class (`CaseNarUnresolvableFoo`), the
      # other doesn't (a `Range`) — the whole clause must stay
      # UNNARROWED, never narrow off of only the conditions that did
      # resolve. `x` keeps its original `CaseNarUnresolvableFoo |
      # CaseNarUnresolvableOther` union, which stays silent by the same
      # "unknown receiver, no diagnostic" rule as any other Union — a
      # buggy partial narrow to `CaseNarUnresolvableFoo` alone would
      # make this a concrete-receiver call and wrongly fire E0101.
      x.nonexistent_method
    end
  end
end
