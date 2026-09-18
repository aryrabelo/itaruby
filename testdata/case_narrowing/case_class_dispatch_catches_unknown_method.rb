class CaseNarDispatchBase
end

class CaseNarDispatchSubA < CaseNarDispatchBase
  def only_on_a
    1
  end
end

class CaseNarDispatchSubB < CaseNarDispatchBase
  def only_on_b
    2
  end
end

class CaseNarDispatchWidget
  def dispatch(event)
    case event
    when CaseNarDispatchSubA
      # `event` narrows to `CaseNarDispatchSubA` inside this branch — an
      # unknown method here is a real E0101.
      event.nonexistent_method
    when CaseNarDispatchSubB
      event.only_on_b
    end
  end
end
