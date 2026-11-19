class CaseNarUnionA
end

class CaseNarUnionB
end

class CaseNarUnionSink
  #: (CaseNarUnionA only_a) -> void
  def take_a(only_a)
  end
end

class CaseNarUnionWidget
  def check(event)
    sink = CaseNarUnionSink.new

    case event
    when CaseNarUnionA, CaseNarUnionB
      # `when A, B` narrows `event` to `A | B` (Module#=== union), not to
      # `A` alone or back to Unknown — hover reads that union. The `#:`
      # sig judges only literal arguments (MATRIX H2), so passing the
      # narrowed local stays silent while the literal below accuses.
      sink.take_a(event)
      sink.take_a(:not_a)
    end
  end
end
