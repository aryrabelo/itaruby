# typed: true
# Open classes: a class reopened later in the same file. The full surface of
# `Invoice` is the UNION of both bodies, and `total_cent` is in neither — a
# real NoMethodError at runtime.
class Invoice
  def initialize(number)
    @number = number
  end

  def number
    @number
  end
end

class Invoice
  def total_cents
    1_000
  end
end

Invoice.new("A-1").total_cent