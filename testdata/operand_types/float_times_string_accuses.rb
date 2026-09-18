# E0108 on a Float receiver: `Float#*` with a String operand is
# `TypeError: String can't be coerced into Float`. Same proof shape as
# the Integer fixture — the receiver's type comes from a Float literal.
class OperandTypesRate
  def scaled
    rate = 1.5
    suffix = "x"
    rate * suffix
  end
end

# Executed by `mri_ground_truth_is_executed`: raises `TypeError: String
# can't be coerced into Float` on the `rate * suffix` line above.
OperandTypesRate.new.scaled
