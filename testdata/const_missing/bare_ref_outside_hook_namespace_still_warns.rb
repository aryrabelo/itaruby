# The cref chain is NOT walked outward. With the hook on `Outer` and the
# bare reference inside `Outer::Inner`, MRI raises
# `uninitialized constant Outer::Inner::Widget` — measured — so the warning
# must survive. Silencing this would buy quiet Ruby does not give.
module Outer
  def self.const_missing(name)
    const_set(name, Struct.new(:x))
  end

  module Inner
    def self.build
      Widget.new(1)
    end
  end
end

Outer::Inner.build
