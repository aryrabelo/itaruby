# Same, wrong arity: MRI raises ArgumentError, itaruby reports E0102.
class Meter
end

class << Meter
  def calibrate(scale)
    scale
  end
end

Meter.calibrate(1, 2)
