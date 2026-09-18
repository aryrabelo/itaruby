class Invoice
  def total
    0
  end
end

# Same call, a lock that names no mocking gem: at runtime this is a
# certain NoMethodError and the checker keeps its conclusive NotFound
# (the population gate is what separates these two fixtures).
Invoice.any_instance
