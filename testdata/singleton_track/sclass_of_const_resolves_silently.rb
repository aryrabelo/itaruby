# `class << X` (a constant, not `self`) defines methods on X's class
# object from a file that never opens X.
class Meter
end

class << Meter
  def calibrate(scale)
    "calibrated:#{scale}"
  end

  attr_accessor :unit
end

Meter.unit = "cm"
raise "expected class-object attr" unless Meter.unit == "cm"
raise "expected class-object def" unless Meter.calibrate(2) == "calibrated:2"
