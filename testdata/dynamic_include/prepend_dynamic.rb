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

class DynamicPrependTarget
  Wrapper.unsafe(self).prepend Registry::Helpers.mixin

  def use_it
    dynamic_only_method
  end
end
