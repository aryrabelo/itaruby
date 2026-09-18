# SILENT control, correct code that uses exactly the same ingredients:
# interpolation is how you legally put an Integer inside a String, and
# every operator pairing here is one MRI accepts. If any of these fires,
# the check is over-reaching.
class OperandTypesCorrect
  def legal
    price = 100
    label = "R$ #{price}"
    interpolated = "#{label} / #{price + 1}"
    interpolated + "!"
  end

  def legal_numeric_mix
    whole = 100
    part = 1.5
    whole + part
  end

  def legal_string_repeat
    label = "ab"
    factor = 2
    label * factor
  end
end

# Executed by `mri_ground_truth_is_executed`: every method here runs
# clean, so this file must exit 0 with nothing raised.
OperandTypesCorrect.new.legal
OperandTypesCorrect.new.legal_numeric_mix
OperandTypesCorrect.new.legal_string_repeat
