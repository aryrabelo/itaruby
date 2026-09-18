# Defect B, qualified top-level path write: referencing
# `ConstVisOuter::ConstVisInner` from another file in the same project.
# Must become silent.
class ConstVisPathReader
  def inner
    ConstVisOuter::ConstVisInner
  end
end
