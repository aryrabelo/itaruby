# SILENT control, the reassignment rule: `price` is written twice with
# DIFFERENT literal types, so the flow-insensitive scan poisons the name
# and the site stays silent even on the path where `price` really is an
# Integer. This is the fail-closed direction — a reassignment ANYWHERE in
# the scope silences every site in it, including sites lexically above
# the second write.
class OperandTypesBranch
  def total(flag)
    price = 100
    price = "free" if flag
    label = "R$"
    price + label
  end
end

# Executed by `mri_ground_truth_is_executed`: `total(true)` runs clean
# ("free" + "R$"), and `total(false)` really raises — the fail-closed
# direction, an accepted false negative, rescued and printed as evidence.
OperandTypesBranch.new.total(true)
begin
  OperandTypesBranch.new.total(false)
rescue TypeError => e
  puts "total: #{e.class}"
end
