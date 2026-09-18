# E0108, the capability's own reference snippet: both operands are proven
# from literals inside ONE method body, nothing reassigns either, and MRI
# really raises `TypeError: String can't be coerced into Integer` on the
# `price + label` line (proven with `ruby -e` in
# crates/itaruby_semantic/tests/operand_types.rs's header).
class OperandTypesPrice
  def label_total
    price = 100
    label = "R$ #{price}"
    price + label
  end
end

# Executed for real by `mri_ground_truth_is_executed` (a body nobody calls
# proves nothing about MRI): this raises `TypeError: String can't be
# coerced into Integer` on the `price + label` line above.
OperandTypesPrice.new.label_total
