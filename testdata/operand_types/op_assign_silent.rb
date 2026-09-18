# SILENT control, the non-`=` binding forms: `+=` (an operator write),
# multi-assignment and a block parameter shadowing the name each poison
# it. None of these locals is provable, so none of the operator sites
# below can fire.
class OperandTypesRebound
  def op_assigned
    price = 100
    price += 1
    label = "R$"
    price + label
  end

  def multi_assigned
    price, label = 100, "R$"
    price + label
  end

  def block_shadowed
    price = 100
    label = "R$"
    [1].each { |price| price + label }
  end
end

# Executed by `mri_ground_truth_is_executed`. Every site here really
# RAISES at runtime — these are itaruby's accepted false negatives, not
# legal code: `+=`, multi-assign and a shadowing block parameter each
# make the local unprovable, and invariant #1 ranks silence above a guess.
# The driver rescues each one so the count is evidence instead of a crash
# on the first.
[:op_assigned, :multi_assigned, :block_shadowed].each do |m|
  begin
    OperandTypesRebound.new.public_send(m)
  rescue TypeError => e
    puts "#{m}: #{e.class}"
  end
end
