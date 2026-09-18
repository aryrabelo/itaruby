# SILENT control, parameters: a parameter's runtime value comes from the
# caller, so `prove_operand_locals` poisons every parameter name even
# when the body also assigns it a literal. Both shapes are here: an
# operand that is only a parameter, and a parameter the body reassigns
# from a literal.
class OperandTypesParams
  def from_param(label)
    price = 100
    price + label
  end

  def reassigned_param(price)
    price = 100
    label = "R$"
    price + label
  end
end

# Executed by `mri_ground_truth_is_executed`: `from_param(1)` runs clean
# (101), while `reassigned_param(0)` really raises — the parameter
# poisoning is an accepted false negative, so the raise is rescued and
# printed as evidence.
OperandTypesParams.new.from_param(1)
begin
  OperandTypesParams.new.reassigned_param(0)
rescue TypeError => e
  puts "reassigned_param: #{e.class}"
end
