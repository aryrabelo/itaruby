class Widget < ApplicationRecord
end

w = Widget.new
w.quantity = 5
w.label = "Acme Widget"
w.active = true
w.quantity
w.label
