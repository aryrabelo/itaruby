class Doohickey < ApplicationRecord
end

def bump(delta)
  w = Doohickey.new
  w.quantity = delta
end
