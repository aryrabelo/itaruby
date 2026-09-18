module Registry
  module Helpers
    def self.mixin
      Module.new
    end
  end
end

module Wrapper
  def self.unsafe(obj)
    obj
  end
end

class UnsafeReceiverMixin
  Wrapper.unsafe(self).include Registry::Helpers.mixin

  def use_it
    dynamic_only_method
  end
end
