# Negative control: a genuinely undefined constant must keep warning. This
# is a real Ruby NameError, not a resolver gap — suppressing it would be
# the exact blanket-suppression failure the defect-A/B fix must avoid.
# Must STILL WARN E0104.
class ConstVisNegative
  def missing
    ConstVisNeverDefinedAnywhere
  end
end
