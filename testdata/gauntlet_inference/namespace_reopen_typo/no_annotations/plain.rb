# typed: true
# Namespace reopening: `Billing::Charge` is opened nested first, then again
# in compact form. Both bodies are the same class; `refundd?` is in neither.
module Billing
  class Charge
    def amount
      0
    end
  end
end

class Billing::Charge
  def refunded?
    false
  end
end

Billing::Charge.new.refundd?