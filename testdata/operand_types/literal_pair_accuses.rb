# E0108 with no locals at all: two literals in one expression prove
# themselves. `1 + "2"` raises the same TypeError MRI gives the
# variable-fed shape — the check reads operand NODES, so a literal needs
# no assignment to be proven.
class OperandTypesLiterals
  def bad_sum
    1 + "2"
  end
end

# Executed by `mri_ground_truth_is_executed`: raises `TypeError: String
# can't be coerced into Integer` on the `1 + "2"` line above.
OperandTypesLiterals.new.bad_sum
