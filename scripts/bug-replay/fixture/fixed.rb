# frozen_string_literal: true

class Invoice
  def total
    100
  end
end

invoice = Invoice.new
invoice.total
