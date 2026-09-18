# Same shape, wrong arity on the prepended class method: MRI raises
# ArgumentError on the call below, and itaruby reports E0102 because the
# method is now INDEXED on Bus's singleton track.
class Bus
  def self.trigger(name)
    name
  end
end

module BusTestHelper
  def track(name)
    trigger(name)
  end
end

Bus.singleton_class.prepend BusTestHelper

Bus.track("x", "y")
