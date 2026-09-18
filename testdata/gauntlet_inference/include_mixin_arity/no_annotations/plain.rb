# typed: true
# include: the mixin's arity is the call's arity. `tax_for` needs two
# arguments and gets one — a real ArgumentError.
module Taxable
  def tax_for(amount, rate)
    amount * rate
  end
end

class Order
  include Taxable
end

Order.new.tax_for(100)