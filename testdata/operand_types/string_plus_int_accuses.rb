# E0108 in the other direction: `String#+` with an Integer operand is
# `TypeError: no implicit conversion of Integer into String`. `String#*`
# is deliberately NOT part of this check (`"ab" * 2` is legal Ruby), so
# only `+` fires here.
class OperandTypesLabel
  def tagged
    label = "total: "
    amount = 42
    label + amount
  end
end

# Executed by `mri_ground_truth_is_executed`: raises `TypeError: no
# implicit conversion of Integer into String` on the `label + amount`
# line above.
OperandTypesLabel.new.tagged
