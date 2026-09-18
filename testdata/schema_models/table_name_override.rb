class Gizmo < ApplicationRecord
  self.table_name = "sprockets"
end

Gizmo.new.count = "xyz"
