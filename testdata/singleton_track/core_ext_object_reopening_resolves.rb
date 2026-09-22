# frozen_string_literal: true

# Bead ita-obx, RESOLUTION side: a class object is an instance of `Class`,
# which is a `Module`, which is an `Object` — so a project reopening of
# `Object` (the exact shape activesupport's core_ext/object/inclusion.rb
# and core_ext/object/with.rb ship) defines a method EVERY class object
# answers. The class-object track must walk past `Class`/`Module` into
# `Object`: `Stone.obx_in?(Gem)` resolves, MRI runs to completion.
class Object
  def obx_in?(other)
    other == self
  end
end

class Stone; end

raise "bad" unless Stone.obx_in?(Stone)
