# E0108 with a `nil` operand: `100 - nil` is
# `TypeError: nil can't be coerced into Integer`. The `nil` here is a
# proven local (written once, from the `nil` literal), not the literal
# itself — the local half of the proof, on the argument side.
class OperandTypesDiscount
  def net
    total = 100
    discount = nil
    total - discount
  end
end

# Executed by `mri_ground_truth_is_executed`: raises `TypeError: nil
# can't be coerced into Integer` on the `total - discount` line above.
OperandTypesDiscount.new.net
