class Widget < ApplicationRecord
end

# schema.rb says `quantity` is `integer` (E0106 risk); structure.sql says
# `text` (no risk). This warning firing proves schema.rb won.
Widget.new.quantity = "abc"
