# frozen_string_literal: true

# Bead ita-obx, ACCUSATION side: the `Object` reopening carries a REAL
# signature, so resolution through it is checkable knowledge and not just
# silence. `obx_with` takes one argument; the zero-argument call on the
# class object is an E0102 MRI really raises.
class Object
  def obx_with(attributes)
    attributes
  end
end

class Pebble; end

Pebble.obx_with
