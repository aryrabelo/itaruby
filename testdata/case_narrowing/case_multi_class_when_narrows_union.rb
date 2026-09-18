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
      # `A` alone or back to Unknown — passing it to a method that only
      # accepts `A` must catch the `B` half as a real E0103.
      sink.take_a(event)
    end
  end
end
