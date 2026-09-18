# The qualified counterpart of the same rule: nesting is not inheritance.
# For `Outer::Inner::Widget` MRI calls `Inner.const_missing`, and `Inner`
# has none — the hook on the grandparent `Outer` never runs. Measured:
# `uninitialized constant Outer::Inner::Widget`. The warning must survive.
module Outer
  def self.const_missing(name)
    const_set(name, Struct.new(:x))
  end

  module Inner
  end
end

Outer::Inner::Widget
